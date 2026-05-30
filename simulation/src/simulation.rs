//! 初期化と実行ドライバ (SimulationBuilder 配線)．

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write as _};

use csv::Writer;
use rand::Rng;

use socsim_core::{derive_seed, AgentId, SimRng};
use socsim_engine::{RandomActivationScheduler, SimulationBuilder};
use socsim_mechanisms::PerAgentThresholdContagionMechanism;
use socsim_net::SocialNetwork;

use crate::config::{Config, DecisionMode, NetworkModel};
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

/// LLM 版 run のエントリ (feature `llm` 必須．非 feature 時はパニック)．
///
/// `cfg.cache_path` を含む [`crate::llm::LlmSettings`] から永続キャッシュ付き本番
/// クライアントを構築し，cache 統計を集計しつつ `(結果, llm_meta JSON)` を返す．
#[cfg(feature = "llm")]
pub fn run_llm_with_meta(cfg: &Config) -> (SimulationResult, serde_json::Value) {
    let settings = crate::llm::settings_from_config(cfg);
    let client = crate::llm::build_live_client_from_settings(&settings)
        .unwrap_or_else(|e| panic!("LLM クライアント構築に失敗: {e}"));
    crate::llm::run_llm_with_meta(cfg, client)
}

/// LLM 版 run のエントリ (feature `llm` 無効時: パニックする案内のみ)．
#[cfg(not(feature = "llm"))]
pub fn run_llm_with_meta(_cfg: &Config) -> (SimulationResult, serde_json::Value) {
    panic!(
        "--decision-mode llm は `--features llm` でビルドした場合のみ利用できます \
         (cargo run --release --features llm -- ...)"
    );
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

/// 設定に応じて rule / llm のドライバを選び，`(結果, llm_meta JSON)` を返す．
///
/// rule モードは LLM 非呼び出しなので meta は `None`．LLM モードは
/// [`run_llm_with_meta`] が `MetadataCollector` から集計した実 cache 統計
/// (model / endpoint / temperature / seed / calls / cache_hits / cache_hit_rate)
/// を `Some(json)` で返す — placeholder ではない実 `llm_meta.json` をここから書く．
pub fn run_dispatch_with_meta(cfg: &Config) -> (SimulationResult, Option<serde_json::Value>) {
    match cfg.decision_mode {
        DecisionMode::Rule => (run(cfg), None),
        DecisionMode::Llm => {
            let (result, meta) = run_llm_with_meta(cfg);
            (result, Some(meta))
        }
    }
}

// ---------------------------------------------------------------------------
// 出力
// ---------------------------------------------------------------------------

/// 意見・表出履歴を long-format CSV に保存する: t, agent_id, b, e, pi_now, pi_fut．
pub fn save_opinions(snapshots: &[Vec<(f64, i8, f64, f64)>], output_dir: &str) {
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

/// メトリクス履歴を CSV に保存する (socsim_results::write_csv に委譲)．
pub fn save_metrics(metrics: &[Metrics], output_dir: &str) {
    let path = format!("{}/metrics.csv", output_dir);
    socsim_results::write_csv(metrics, &path).expect("metrics.csv の書き込みに失敗");
}

/// 出力ディレクトリを作成する．
pub fn ensure_output_dir(output_dir: &str) {
    socsim_results::ensure_dir(output_dir).expect("出力ディレクトリの作成に失敗");
}

/// 任意の JSON 値をファイルへ書く薄ラッパ (llm_meta.json 等)．
pub fn write_json_file(value: &serde_json::Value, path: &str) {
    let file = File::create(path).expect("JSON ファイルの作成に失敗");
    let mut w = BufWriter::new(file);
    let s = serde_json::to_string_pretty(value).expect("JSON 直列化に失敗");
    w.write_all(s.as_bytes()).expect("JSON 書き込みに失敗");
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
