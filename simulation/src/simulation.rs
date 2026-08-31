//! 初期化と実行ドライバ (SimulationBuilder 配線)．

use std::collections::VecDeque;
use std::fs::File;
use std::io::BufWriter;

use csv::Writer;
use rand::Rng;

use socsim_core::{derive_seed, AgentId, SimRng};
use socsim_engine::{RandomActivationScheduler, SimulationBuilder};
use socsim_mechanisms::PerAgentThresholdContagionMechanism;
use socsim_net::SocialNetwork;

use crate::config::{Config, NetworkModel};
use crate::mechanisms::{
    ClimateQuasiStatMechanism, FearAppraisalMechanism, FutureAssessmentMechanism,
    IssueSalienceMechanism, MediaSignalMechanism, RuleOracle, SilenceSpiralMechanism,
    VoiceDecisionMechanism, VoiceOracle,
};
use crate::metrics::Metrics;
use crate::world::{Expression, SpiralWorld};

/// 初期 `b_i, f_i, θ_i, network` 生成用 RNG ラベル．
const RNG_WORLD_INIT: u64 = 0;
/// socsim エンジン (scheduler / prefalse_cascade のベルヌーイ抽選) 用 RNG ラベル．
const RNG_ENGINE: u64 = 1;
/// media_signal の確率的シグナル生成用 RNG ラベル．
const RNG_MEDIA: u64 = 2;

/// シミュレーション結果．
pub struct SimulationResult {
    /// 各 tick (t=0 含む) のメトリクス履歴．
    pub metrics_history: Vec<Metrics>,
    /// 各 tick の (b, e, pi_now, pi_fut) スナップショット (opinions.csv 用)．
    /// `snapshots[t][i] = (b, e_code, pi_now, pi_fut)`．
    pub snapshots: Vec<Vec<(f64, i8, f64, f64)>>,
    /// 収束したか．
    pub converged: bool,
    /// 最終 tick．
    pub final_tick: usize,
    /// 使用した root シード．
    pub seed: u64,
}

/// 設定と root シードから初期世界を生成する．
pub fn init_world(cfg: &Config, root: u64) -> SpiralWorld {
    let mut rng = SimRng::from_seed(derive_seed(root, &[RNG_WORLD_INIT]));
    let n = cfg.n;
    let ids: Vec<AgentId> = (0..n as u64).map(AgentId).collect();

    // 網生成．
    let network = match cfg.network_model {
        NetworkModel::WattsStrogatz => {
            SocialNetwork::watts_strogatz(&ids, cfg.network_k, cfg.network_beta, &mut rng)
        }
        NetworkModel::ErdosRenyi => {
            // 期待次数 k → p = k/(n-1)．
            let p = if n > 1 {
                cfg.network_k as f64 / (n as f64 - 1.0)
            } else {
                0.0
            };
            SocialNetwork::erdos_renyi(&ids, p, &mut rng)
        }
        NetworkModel::BarabasiAlbert => {
            SocialNetwork::barabasi_albert(&ids, cfg.network_k.max(1) / 2, &mut rng)
        }
    };

    // 私的意見 b_i: 真の賛成支持率 q に従い符号を割り当て，大きさは [0.3,1.0] 一様．
    // **網位相とは独立に**賛成/反対をランダム配置する (WS の ring lattice は隣接 ID を
    // 結ぶため，連続 ID で陣営を割り当てると人為的な同類選好が生じ気候知覚が歪む)．
    // そこで陣営フラグ列を Fisher--Yates でシャッフルしてから割り当てる．
    let n_pro = (cfg.true_support * n as f64).round() as usize;
    let mut is_pro: Vec<bool> = (0..n).map(|i| i < n_pro).collect();
    for i in (1..n).rev() {
        let j = rng.gen_range(0..=i);
        is_pro.swap(i, j);
    }
    let mut b_priv = vec![0.0f64; n];
    for (i, b) in b_priv.iter_mut().enumerate() {
        let mag = rng.gen_range(0.3..1.0);
        *b = if is_pro[i] { mag } else { -mag };
    }

    // 孤立恐怖 f_i ∈ [0.2,0.8] 一様．
    let fear: Vec<f64> = (0..n).map(|_| rng.gen_range(0.2..0.8)).collect();

    // 表明閾値 θ_i ∈ [0,1] 一様 → 下裾 hardcore_frac をハードコア (θ を 0 付近に圧縮)．
    let mut voice_threshold: Vec<f64> = (0..n).map(|_| rng.gen_range(0.0..1.0)).collect();
    let n_hardcore = (cfg.hardcore_frac * n as f64).round() as usize;
    // 閾値の昇順下裾 n_hardcore 個を 0 付近へ (発言を拒まないハードコア)．
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| voice_threshold[a].partial_cmp(&voice_threshold[b]).unwrap());
    for &i in order.iter().take(n_hardcore) {
        voice_threshold[i] *= 0.1;
    }

    // 初期気候: 各自の自意見側多数度を真の支持率から見積もる (賛成派は q，反対派は 1-q)．
    let q = cfg.true_support;
    let pi_now: Vec<f64> = b_priv
        .iter()
        .map(|&b| if b >= 0.0 { q } else { 1.0 - q })
        .collect();
    let pi_fut = pi_now.clone();

    // 初期表出: 沈黙から開始 (t=0 で全員沈黙，最初の voice_decision で発言が立つ)．
    let e_pub = vec![Expression::Silence; n];
    let voice_history = vec![VecDeque::new(); n];

    SpiralWorld {
        clock: socsim_core::SimClock::new(cfg.t_max as u64),
        agents: ids,
        network,
        b_priv,
        e_pub,
        pi_now,
        pi_fut,
        voice_history,
        fear,
        voice_threshold,
        media_signal: 0.0,
        media_homogeneity: cfg.eta_m,
    }
}

/// 世界スナップショットを (b, e_code, pi_now, pi_fut) のベクタへ変換する．
fn snapshot(world: &SpiralWorld) -> Vec<(f64, i8, f64, f64)> {
    (0..world.n())
        .map(|i| {
            (
                world.b_priv[i],
                world.e_pub[i].code(),
                world.pi_now[i],
                world.pi_fut[i],
            )
        })
        .collect()
}

/// メディアが推す側の符号 (真の多数派側を増幅する想定)．
fn media_target_sign(cfg: &Config) -> f64 {
    if cfg.true_support >= 0.5 {
        1.0
    } else {
        -1.0
    }
}

/// ルールベース版でシミュレーションを実行する (LLM 非呼び出し)．
pub fn run(cfg: &Config) -> SimulationResult {
    let root = cfg.seed.unwrap_or_else(rand::random);
    let oracle = RuleOracle {
        beta_0: cfg.beta_0,
        beta_b: cfg.beta_b,
        beta_u: cfg.beta_u,
        beta_fear: cfg.beta_fear,
        beta_pi: cfg.beta_pi,
        beta_theta: cfg.beta_theta,
        alpha: cfg.alpha,
        alpha_a: cfg.alpha_a,
    };
    run_with_oracle(cfg, root, oracle)
}

/// 任意の [`VoiceOracle`] でシミュレーションを駆動する共通ドライバ．
///
/// ルール版は [`run`]，LLM 版は `crate::llm::run_with_client` から呼ばれる．media_signal の
/// 確率ストリームは RNG_MEDIA を使うため，本来の engine RNG (RNG_ENGINE) と分離する
/// 必要があるが，socsim エンジンは単一 RNG をメカニズムに供給する．そこで
/// media_signal は engine RNG から引き，RNG_MEDIA は seed 導出のラベルとして
/// `MediaSignalMechanism` には渡さず，決定論性は engine seed (RNG_ENGINE) に集約する．
pub fn run_with_oracle<O: VoiceOracle + 'static>(
    cfg: &Config,
    root: u64,
    oracle: O,
) -> SimulationResult {
    let world = init_world(cfg, root);
    let media_sign = media_target_sign(cfg);

    // engine seed は RNG_ENGINE と RNG_MEDIA を畳み込んで決定論化する．
    let engine_seed = derive_seed(root, &[RNG_ENGINE, RNG_MEDIA]);

    let mut builder = SimulationBuilder::new(world)
        .scheduler(Box::new(RandomActivationScheduler))
        .seed(engine_seed)
        // Environment
        .add_mechanism(Box::new(MediaSignalMechanism {
            eta_m: cfg.eta_m,
            target_sign: media_sign,
        }))
        .add_mechanism(Box::new(IssueSalienceMechanism { salience: 1.0 }))
        // Decision
        .add_mechanism(Box::new(FearAppraisalMechanism { mu: 0.3 }))
        .add_mechanism(Box::new(FutureAssessmentMechanism { gamma: cfg.gamma }))
        .add_mechanism(Box::new(VoiceDecisionMechanism { oracle }))
        // Interaction
        .add_mechanism(Box::new(SilenceSpiralMechanism { lambda: cfg.lambda }));
    // prefalse_cascade: PerAgentThresholdContagion 直接流用
    // (BinaryState+Neighbors+ActivationThreshold)．各沈黙者 i を世界状態の per-agent
    // 閾値 θ_i (= voice_threshold[i]) と比べ，近傍発言比 ρ^V_i ≥ θ_i の沈黙者を発言へ
    // 反転させる．低 θ_i のハードコア (下裾) は飽和近傍を待たず動員され，高 θ_i の
    // 一般層は大域フラッディングに巻き込まれない — これで沈黙の螺旋非対称を保つ．
    builder = builder.add_mechanism(Box::new(PerAgentThresholdContagionMechanism::new()));
    // PostStep
    builder = builder.add_mechanism(Box::new(ClimateQuasiStatMechanism::new(cfg.window, 1e-3)));

    let mut sim = builder.build();

    let mut metrics_history: Vec<Metrics> = Vec::new();
    let mut snapshots: Vec<Vec<(f64, i8, f64, f64)>> = Vec::new();

    // 初期状態 (t=0)．
    metrics_history.push(Metrics::compute(sim.world(), 0));
    snapshots.push(snapshot(sim.world()));

    let mut converged = false;
    let mut final_tick = 0usize;

    sim.run_observed(|report| {
        let t = report.t as usize;
        metrics_history.push(Metrics::compute(report.world, t));
        snapshots.push(snapshot(report.world));
        converged = report.stopped;
        final_tick = t;
    })
    .expect("シミュレーションの実行に失敗");

    SimulationResult {
        metrics_history,
        snapshots,
        converged,
        final_tick,
        seed: root,
    }
}

/// LLM バックエンドの同一性 (`run.json` の `llm` ブロックの材料)．
///
/// 実行前に分かる値だけを持つ．どのモデルがどの endpoint で答えるのかを知っているのは
/// クライアントを組んだ側だけなので，run を開始する前にここまで確定させる．
#[derive(Debug, Clone)]
pub struct LlmIdentity {
    /// バックエンドが名乗ったモデル名．
    pub model: String,
    /// バックエンドが名乗った endpoint．
    pub endpoint: String,
    /// 生成温度．
    pub temperature: f32,
}

/// LLM 呼び出しの内訳 (旧 `llm_meta.json` の calls / cache_hits / cache_hit_rate)．
///
/// 実行後にしか分からない値だけを持つ．
#[derive(Debug, Clone)]
pub struct LlmUsage {
    /// 呼び出し回数．
    pub calls: usize,
    /// うちキャッシュ命中数．
    pub cache_hits: usize,
    /// キャッシュ命中率．
    pub cache_hit_rate: f64,
}

/// 組み立て済みの LLM クライアントと，そのバックエンドの同一性．
///
/// feature `llm` 無効時は中身を持たない型になる．そちらの `prepare_llm` は値を作る前に
/// 必ずパニックするので，この型の値はそもそも存在しない — 呼び出し側は feature の
/// 有無を意識せずに `Option<PreparedLlm>` を組み立てられる．
#[cfg(feature = "llm")]
pub struct PreparedLlm {
    client: crate::llm::LiveClient,
    identity: LlmIdentity,
}

/// 組み立て済みの LLM クライアント (feature `llm` 無効時: 中身を持たない)．
#[cfg(not(feature = "llm"))]
pub struct PreparedLlm(());

/// LLM クライアントを組み，バックエンドの同一性だけ先に取り出す (feature `llm` 必須)．
///
/// `cfg.cache_path` を含む [`crate::llm::LlmSettings`] から永続キャッシュ付きの本番
/// クライアント (Ollama 第一 → OpenAI フォールバック) を構築する．
#[cfg(feature = "llm")]
pub fn prepare_llm(cfg: &Config) -> PreparedLlm {
    let settings = crate::llm::settings_from_config(cfg);
    let client = crate::llm::build_live_client_from_settings(&settings)
        .unwrap_or_else(|e| panic!("LLM クライアント構築に失敗: {e}"));
    PreparedLlm::new(client, cfg.llm_temperature)
}

/// LLM クライアントの組み立て (feature `llm` 無効時: パニックする案内のみ)．
#[cfg(not(feature = "llm"))]
pub fn prepare_llm(_cfg: &Config) -> PreparedLlm {
    panic!(
        "--decision-mode llm は `--features llm` でビルドした場合のみ利用できます \
         (cargo run --release --features llm -- ...)"
    );
}

#[cfg(feature = "llm")]
impl PreparedLlm {
    /// 任意のクライアント (本番 / mock) から組み立てる．
    pub fn new(client: crate::llm::LiveClient, temperature: f32) -> Self {
        let identity = crate::llm::identity_of(&client, temperature);
        PreparedLlm { client, identity }
    }

    /// バックエンドの同一性．
    pub fn identity(&self) -> &LlmIdentity {
        &self.identity
    }
}

#[cfg(not(feature = "llm"))]
impl PreparedLlm {
    /// バックエンドの同一性 ([`prepare_llm`] が先にパニックするので到達しない)．
    pub fn identity(&self) -> &LlmIdentity {
        unreachable!("feature `llm` 無効時に PreparedLlm は作られない")
    }
}

/// 組み立て済みクライアントで LLM 版を駆動する (feature `llm` 必須)．
#[cfg(feature = "llm")]
pub fn run_prepared(cfg: &Config, prepared: PreparedLlm) -> (SimulationResult, LlmUsage) {
    crate::llm::run_llm_with_usage(cfg, prepared.client)
}

/// 組み立て済みクライアントで LLM 版を駆動する (feature `llm` 無効時: 到達しない)．
#[cfg(not(feature = "llm"))]
pub fn run_prepared(_cfg: &Config, _prepared: PreparedLlm) -> (SimulationResult, LlmUsage) {
    unreachable!("feature `llm` 無効時に PreparedLlm は作られない")
}

/// ルール版を実行し，**最終 tick の世界状態**を返す (陣営別指標の集計用)．
///
/// `run` は (CSV 向けに) スナップショットのみ返すため，`reproduce` で陣営別
/// `voice_by_camp` / `pi_now_by_camp` / `hardcore_survival` を最終世界で測りたい
/// 場合にこちらを使う．ドライバ本体と同一の配線・seed なので `run` と同一軌跡．
pub fn final_world(cfg: &Config) -> SpiralWorld {
    let root = cfg.seed.unwrap_or_else(rand::random);
    let oracle = RuleOracle {
        beta_0: cfg.beta_0,
        beta_b: cfg.beta_b,
        beta_u: cfg.beta_u,
        beta_fear: cfg.beta_fear,
        beta_pi: cfg.beta_pi,
        beta_theta: cfg.beta_theta,
        alpha: cfg.alpha,
        alpha_a: cfg.alpha_a,
    };
    let world = init_world(cfg, root);
    let media_sign = media_target_sign(cfg);
    let engine_seed = derive_seed(root, &[RNG_ENGINE, RNG_MEDIA]);

    let mut sim = SimulationBuilder::new(world)
        .scheduler(Box::new(RandomActivationScheduler))
        .seed(engine_seed)
        .add_mechanism(Box::new(MediaSignalMechanism {
            eta_m: cfg.eta_m,
            target_sign: media_sign,
        }))
        .add_mechanism(Box::new(IssueSalienceMechanism { salience: 1.0 }))
        .add_mechanism(Box::new(FearAppraisalMechanism { mu: 0.3 }))
        .add_mechanism(Box::new(FutureAssessmentMechanism { gamma: cfg.gamma }))
        .add_mechanism(Box::new(VoiceDecisionMechanism { oracle }))
        .add_mechanism(Box::new(SilenceSpiralMechanism { lambda: cfg.lambda }))
        .add_mechanism(Box::new(PerAgentThresholdContagionMechanism::new()))
        .add_mechanism(Box::new(ClimateQuasiStatMechanism::new(cfg.window, 1e-3)))
        .build();
    sim.run().expect("シミュレーションの実行に失敗");
    sim.world().clone()
}

// ---------------------------------------------------------------------------
// 出力
// ---------------------------------------------------------------------------

/// 意見・表出履歴を long-format CSV に保存する: t, agent_id, b, e, pi_now, pi_fut．
///
/// 置き場は runvault の run ディレクトリの `artifacts/` 配下で，呼び出し側が渡す．
/// `finish()` が `artifacts/` と `logs/` を歩いて `manifest.csv` を作るので，run が
/// 終わった後にここへ足したものは記録に載らない．
pub fn save_opinions(snapshots: &[Vec<(f64, i8, f64, f64)>], output_dir: &str) {
    std::fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");
    let path = format!("{}/opinions.csv", output_dir);
    let file = File::create(&path).expect("opinions.csv の作成に失敗");
    let mut wtr = Writer::from_writer(BufWriter::new(file));
    wtr.write_record(["t", "agent_id", "b", "e", "pi_now", "pi_fut"])
        .expect("ヘッダ書き込みに失敗");
    for (t, snap) in snapshots.iter().enumerate() {
        for (i, &(b, e, pi_now, pi_fut)) in snap.iter().enumerate() {
            wtr.write_record(&[
                t.to_string(),
                i.to_string(),
                format!("{:.6}", b),
                e.to_string(),
                format!("{:.6}", pi_now),
                format!("{:.6}", pi_fut),
            ])
            .expect("レコード書き込みに失敗");
        }
    }
    wtr.flush().expect("フラッシュに失敗");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            n: 200,
            t_max: 30,
            seed: Some(42),
            ..Config::default()
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let a = run(&test_config());
        let b = run(&test_config());
        assert_eq!(a.final_tick, b.final_tick);
        let la = a.snapshots.last().unwrap();
        let lb = b.snapshots.last().unwrap();
        assert_eq!(la.len(), lb.len());
        for (x, y) in la.iter().zip(lb.iter()) {
            assert_eq!(x.1, y.1, "公的表出が同一シードで一致すべき");
            assert!((x.0 - y.0).abs() < 1e-12);
        }
    }

    #[test]
    fn initial_state_recorded_at_t0() {
        let r = run(&test_config());
        assert_eq!(r.metrics_history[0].t, 0);
        assert_eq!(r.snapshots[0].len(), 200);
    }

    #[test]
    fn network_models_build() {
        for model in [
            NetworkModel::WattsStrogatz,
            NetworkModel::ErdosRenyi,
            NetworkModel::BarabasiAlbert,
        ] {
            let cfg = Config {
                n: 100,
                network_model: model,
                t_max: 10,
                seed: Some(7),
                ..Config::default()
            };
            let w = init_world(&cfg, 7);
            assert_eq!(w.n(), 100);
        }
    }
}
