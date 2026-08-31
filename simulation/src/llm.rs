//! Phase-3 LLM 内省版発言決定 (feature `llm` 時のみコンパイル)．
//!
//! 本モジュールは設計書 §7 の要請どおり **`socsim-llm` 共有ハーネスの薄い
//! re-export シム**である (自前 HTTP クライアントは書かない)．`LiveClient`
//! (`CachingClient<Box<dyn LlmClient>>`)・`llm_config`・`wrap_client`・
//! `build_live_client_from_settings` をそのまま再エクスポートし，`chuang2024` と
//! 同方式の Ollama 第一 → OpenAI フォールバック + プロンプトキャッシュ + 温度0 で
//! 擬似決定論化する．
//!
//! LLM 層は socsim の bit 再現性の外側にある (プロンプト→応答キャッシュで擬似決定
//! 論)．[`run_llm_with_usage`] が `simulation::run_with_oracle` に [`LlmOracle`] を
//! 渡して駆動する．

use std::cell::RefCell;
use std::rc::Rc;

// ── socsim-llm 共有ハーネスの re-export シム ──────────────────────────────────
pub use socsim_llm::build_live_client_from_settings;
use socsim_llm::LlmClient;
pub use socsim_llm::{llm_config, wrap_client, LiveClient, LlmSettings, MetadataCollector};

use crate::config::Config;
use crate::mechanisms::VoiceOracle;
use crate::prompts;
use crate::simulation::{run_with_oracle, LlmIdentity, LlmUsage, SimulationResult};
use crate::world::SpiralWorld;

/// LLM 内省でエージェントの発言確率を返すオラクル．
///
/// クライアント・メタデータを `Rc<RefCell<…>>` で共有し，run 後に cache-hit 率等を
/// 集計できるようにする (`chuang2024` と同方式)．
pub struct LlmOracle {
    client: Rc<RefCell<LiveClient>>,
    metadata: Rc<RefCell<MetadataCollector>>,
    settings: LlmSettings,
    future_weight: f64,
}

impl LlmOracle {
    pub fn new(
        client: Rc<RefCell<LiveClient>>,
        metadata: Rc<RefCell<MetadataCollector>>,
        settings: LlmSettings,
        future_weight: f64,
    ) -> Self {
        LlmOracle {
            client,
            metadata,
            settings,
            future_weight,
        }
    }
}

impl VoiceOracle for LlmOracle {
    fn voice_prob(&mut self, world: &SpiralWorld, i: usize, _cascade_pressure: bool) -> f64 {
        let prompt = prompts::voice_introspection_prompt(
            world.b_priv[i],
            world.pi_now[i],
            world.pi_fut[i],
            world.fear[i],
            world.media_signal,
            self.future_weight,
        );
        let mut client = self.client.borrow_mut();
        match client.complete(&prompt, &llm_config(&self.settings)) {
            Ok(resp) => {
                self.metadata.borrow_mut().record(resp.metadata.clone());
                prompts::parse_voice_response(&resp.text)
            }
            // LLM 失敗時は中立 (沈黙寄り) にフォールバック．
            Err(_) => 0.5,
        }
    }
}

/// `Config` の LLM 層フィールドから [`LlmSettings`] を組み立てる．
///
/// `cache_path: Some(...)` のとき `build_live_client_from_settings` は **永続
/// ファイルキャッシュ**を開くため，プロセスをまたいだ温暖キャッシュ再生
/// (cold→warm 100% cache-hit) が成立する (`cache_path: None` は in-memory)．
pub fn settings_from_config(cfg: &Config) -> LlmSettings {
    LlmSettings {
        temperature: cfg.llm_temperature,
        seed: cfg.llm_seed,
        cache_path: cfg.cache_path.clone(),
    }
}

/// クライアントが名乗ったバックエンドの同一性を取り出す．
///
/// `run.json` の `llm` ブロックの材料になる．推測はしない — モデル名も endpoint も
/// クライアント自身が名乗った値をそのまま読む．
pub fn identity_of(client: &LiveClient, temperature: f32) -> LlmIdentity {
    LlmIdentity {
        model: client.inner().model().to_string(),
        endpoint: client.inner().endpoint().to_string(),
        temperature,
    }
}

/// 与えられた [`LiveClient`] で LLM 版を駆動する (本番 / mock 共通)．
///
/// 本番は [`build_live_client_from_settings`] の結果を，テストは
/// [`wrap_client`] でラップした `mock::ScriptedClient` を渡す．`Config` の
/// 温度・シード設定をオラクルへスレッドする (キャッシュ永続化は client 側が担う)．
pub fn run_with_client(cfg: &Config, client: LiveClient) -> SimulationResult {
    let (result, _usage) = run_llm_with_usage(cfg, client);
    result
}

/// LLM 版を実行し，`MetadataCollector` が集計した実 cache 統計も返す．
///
/// オラクルには `Config` 由来の [`LlmSettings`] (温度・シード) をスレッドする
/// (従来は `LlmSettings::default()` が固定で，設定した温度/シードが効かなかった)．
/// model / endpoint / temperature は実行前に決まるので [`identity_of`] の担当で，
/// ここが返すのは実行しないと分からない呼び出しの内訳だけである．
pub fn run_llm_with_usage(cfg: &Config, client: LiveClient) -> (SimulationResult, LlmUsage) {
    let root = cfg.seed.unwrap_or_else(rand::random);
    let settings = settings_from_config(cfg);
    let shared_client = Rc::new(RefCell::new(client));
    let shared_meta = Rc::new(RefCell::new(MetadataCollector::new()));
    let oracle = LlmOracle::new(
        Rc::clone(&shared_client),
        Rc::clone(&shared_meta),
        settings.clone(),
        cfg.alpha,
    );
    let result = run_with_oracle(cfg, root, oracle);

    // 永続キャッシュは load-on-open / save-on-demand なので，run 後に明示的に
    // `.save()` しないとファイルが書かれず cold→warm 再生が成立しない．
    // `cache_path: None` (mock / in-memory) のときはスキップする．oracle は
    // `run_with_oracle` の return で drop 済みのため素の borrow で衝突しない．
    if cfg.cache_path.is_some() {
        shared_client
            .borrow()
            .cache()
            .save()
            .unwrap_or_else(|e| eprintln!("warning: cache save failed: {e}"));
    }

    let meta = shared_meta.borrow();
    let usage = LlmUsage {
        calls: meta.total(),
        cache_hits: meta.cache_hits(),
        cache_hit_rate: meta.cache_hit_rate(),
    };
    drop(meta);
    (result, usage)
}
