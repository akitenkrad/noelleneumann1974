//! Noelle-Neumann (1974) "The Spiral of Silence" — 再現実験の CLI エントリポイント．
//!
//! `run`       : 単一条件で「沈黙の螺旋」を実行する (rule / llm)．
//! `sweep`     : η_m / network_beta / α 等を走査して境界条件を集計する．
//!               条件 1 点ごとに子 run を起こし，その中で `runs` 本の試行を回す．
//! `reproduce` : §5 のアレンスバッハ調査 Table 1--5 アンカーと baseline 出力を照合する．
//!
//! 出力の置き場と同一性は runvault が持つ．タイムスタンプ付きディレクトリも
//! `latest` シンボリックリンクもこちらでは作らず，`Run::start` が決めた run
//! ディレクトリへ書く．

use clap::{Parser, Subcommand};
use runvault::{Lineage, Run, RunOptions};
use serde::Serialize;

use noelleneumann_spiral_simulation::config::{
    parse_decision_mode, parse_network_model, Config, DecisionMode, RunParameters,
};
use noelleneumann_spiral_simulation::metrics::{pi_now_by_camp, voice_by_camp, Metrics};
use noelleneumann_spiral_simulation::record::{
    self, Anchor, TrialOutcome, DOMAIN, EXPERIMENT, REPO_ID,
};
use noelleneumann_spiral_simulation::simulation::{
    final_world, prepare_llm, run, run_prepared, save_opinions, LlmUsage, PreparedLlm,
    SimulationResult,
};

// ---------------------------------------------------------------------------
// CLI 定義
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "noelleneumann",
    about = "Noelle-Neumann (1974) The Spiral of Silence — 再現実験"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Ollama 接続先 URL（指定時は環境変数 OLLAMA_HOST を上書きする）．
    #[arg(long, global = true)]
    ollama_host: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 単一条件で「沈黙の螺旋」を実行する．
    Run(RunArgs),
    /// パラメータ走査 (η_m / network_beta / α / network_k / true_support)．
    Sweep(SweepArgs),
    /// §5 の Table 1--5 アンカーと baseline 出力を照合する．
    Reproduce(ReproduceArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// エージェント数 n．
    #[arg(long, default_value_t = 1000)]
    n: usize,
    /// 真の賛成支持率 q．
    #[arg(long, default_value_t = 0.37)]
    true_support: f64,
    /// 網モデル (watts-strogatz / erdos-renyi / barabasi-albert)．
    #[arg(long, default_value = "watts-strogatz")]
    network_model: String,
    /// 平均次数 k．
    #[arg(long, default_value_t = 6)]
    network_k: usize,
    /// WS 再配線率 β．
    #[arg(long, default_value_t = 0.1)]
    network_beta: f64,
    /// 媒体均質性 η_m．
    #[arg(long, default_value_t = 0.5)]
    eta_m: f64,
    /// 未来重み α (>0.5)．
    #[arg(long, default_value_t = 0.7)]
    alpha: f64,
    /// 気候係数 β_π．
    #[arg(long, default_value_t = 3.5)]
    beta_pi: f64,
    /// 恐怖係数 β_f．
    #[arg(long, default_value_t = 2.5)]
    beta_fear: f64,
    /// 匿名係数 α_a．
    #[arg(long, default_value_t = 0.0)]
    alpha_a: f64,
    /// ハードコア比率．
    #[arg(long, default_value_t = 0.05)]
    hardcore_frac: f64,
    /// 最大 tick 数 T．
    #[arg(long, default_value_t = 80)]
    t_max: usize,
    /// 発言決定モード (rule / llm; llm は --features llm 必須)．
    #[arg(long, default_value = "rule")]
    decision_mode: String,
    /// LLM 生成温度 (llm モードのみ．既定 0.0 = 擬似決定論)．
    #[arg(long, default_value_t = 0.0)]
    llm_temperature: f32,
    /// LLM 生成シード (llm モードのみ．バックエンドへ渡す)．
    #[arg(long, default_value_t = 0)]
    llm_seed: u64,
    /// プロンプト→応答キャッシュの永続パス (llm モードのみ; rule モードは無視)．
    /// プロセスをまたいで温暖キャッシュ再生 (cold→warm 100% cache-hit) させる．
    #[arg(long, default_value = ".llm_cache/cache.json")]
    cache_path: String,
    /// 乱数シード (省略時はここで実体化して記録する)．
    #[arg(long)]
    seed: Option<u64>,
    /// runvault の results ルート (run ディレクトリの名前は runvault が決める)．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

impl RunArgs {
    /// 実体化したシードで `Config` を組む．出力先は `Run::start` が run ディレクトリを
    /// 決めた後に確定するので，ここでは空にしておく．
    fn to_config(&self, seed: u64) -> Config {
        let network_model =
            parse_network_model(&self.network_model).unwrap_or_else(|e| panic!("{e}"));
        let decision_mode =
            parse_decision_mode(&self.decision_mode).unwrap_or_else(|e| panic!("{e}"));
        Config {
            n: self.n,
            true_support: self.true_support,
            network_model,
            network_k: self.network_k,
            network_beta: self.network_beta,
            eta_m: self.eta_m,
            alpha: self.alpha,
            beta_pi: self.beta_pi,
            beta_fear: self.beta_fear,
            alpha_a: self.alpha_a,
            hardcore_frac: self.hardcore_frac,
            t_max: self.t_max,
            decision_mode,
            seed: Some(seed),
            output_dir: String::new(),
            llm_temperature: self.llm_temperature,
            llm_seed: self.llm_seed,
            // rule モードでは cache_path は無視されるため None に倒す
            // (llm モードのみ永続ファイルキャッシュを開く)．
            cache_path: if decision_mode == DecisionMode::Llm {
                Some(self.cache_path.clone())
            } else {
                None
            },
            ..Config::default()
        }
    }
}

#[derive(Parser, Debug)]
struct SweepArgs {
    /// η_m 候補 (カンマ区切り)．
    #[arg(long, default_value = "0.0,0.25,0.5,0.75,1.0")]
    eta_m_values: String,
    /// network_beta 候補 (カンマ区切り)．
    #[arg(long, default_value = "0.0,0.05,0.1,0.3")]
    network_beta_values: String,
    /// α 候補 (カンマ区切り)．
    #[arg(long, default_value = "0.5,0.6,0.7,0.8")]
    alpha_values: String,
    /// network_k 候補 (カンマ区切り)．
    #[arg(long, default_value = "6")]
    network_k_values: String,
    /// true_support 候補 (カンマ区切り)．
    #[arg(long, default_value = "0.37")]
    true_support_values: String,
    /// 各条件あたりの反復数 (seed 派生)．
    #[arg(long, default_value_t = 30)]
    runs: usize,
    /// エージェント数 n．
    #[arg(long, default_value_t = 1000)]
    n: usize,
    /// hardcore 比率．
    #[arg(long, default_value_t = 0.05)]
    hardcore_frac: f64,
    /// 最大 tick 数．
    #[arg(long, default_value_t = 80)]
    t_max: usize,
    /// シード基点．
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// runvault の results ルート．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

#[derive(Parser, Debug)]
struct ReproduceArgs {
    /// エージェント数 n．
    #[arg(long, default_value_t = 1000)]
    n: usize,
    /// 最大 tick 数．
    #[arg(long, default_value_t = 80)]
    t_max: usize,
    /// シード基点 (シナリオごとに派生)．
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// runvault の results ルート．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

// ---------------------------------------------------------------------------
// 実験条件 (runvault の config.json の parameters)
// ---------------------------------------------------------------------------

/// スイープ親の実験条件．走査するグリッドの定義そのもの．
#[derive(Serialize)]
struct SweepParameters {
    eta_m_values: Vec<f64>,
    network_beta_values: Vec<f64>,
    alpha_values: Vec<f64>,
    network_k_values: Vec<usize>,
    true_support_values: Vec<f64>,
    runs: usize,
    n: usize,
    hardcore_frac: f64,
    t_max: usize,
    seed: u64,
}

/// スイープの子 run (格子点 1 つ) の実験条件．
///
/// `run` の条件に `runs` が付いた形で，`run` とは別のサブコマンド名を持つ．同じ `run`
/// を名乗らせると，「1 本のシミュレーション」と「同一条件の `runs` 本」という中身の
/// 違う 2 つが 1 つの名前に同居し，`runvault path --subcommand run` がどちらを返すか
/// 分からなくなる．
#[derive(Serialize)]
struct SweepPointParameters {
    n: usize,
    true_support: f64,
    network_k: usize,
    network_beta: f64,
    eta_m: f64,
    alpha: f64,
    hardcore_frac: f64,
    t_max: usize,
    runs: usize,
    seed: u64,
}

/// `reproduce` の実験条件．
///
/// baseline シナリオ (社会主義到来 q=0.37) の条件をそのまま持ち，ハードコア境界
/// シナリオが変える 1 つのパラメータだけを足す．2 つのシナリオは «同じ実行の中の
/// 比較» なので，子 run には分けない．
#[derive(Serialize)]
struct ReproduceParameters {
    #[serde(flatten)]
    baseline: RunParameters,
    /// ハードコア境界シナリオの hardcore 比率 (baseline は 0.05)．
    boundary_hardcore_frac: f64,
}

// ---------------------------------------------------------------------------
// 補助
// ---------------------------------------------------------------------------

fn parse_f64_list(s: &str) -> Vec<f64> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| {
            x.parse::<f64>()
                .unwrap_or_else(|_| panic!("数値のパースに失敗: '{x}'"))
        })
        .collect()
}

fn parse_usize_list(s: &str) -> Vec<usize> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| {
            x.parse::<usize>()
                .unwrap_or_else(|_| panic!("整数のパースに失敗: '{x}'"))
        })
        .collect()
}

/// 定常状態 (後半平均) のメトリクスを返す (t > T/2 の平均)．
fn steady_state(result: &SimulationResult) -> Metrics {
    let hist = &result.metrics_history;
    let start = hist.len() / 2;
    let slice = &hist[start..];
    let n = slice.len().max(1) as f64;
    let mean = |f: fn(&Metrics) -> f64| slice.iter().map(f).sum::<f64>() / n;
    Metrics {
        t: hist.last().map(|m| m.t).unwrap_or(0),
        voice_volume: mean(|m| m.voice_volume),
        majority_voice_ratio: mean(|m| m.majority_voice_ratio),
        perceived_minus_actual: mean(|m| m.perceived_minus_actual),
        future_assessment_gap: mean(|m| m.future_assessment_gap),
        hardcore_survival: mean(|m| m.hardcore_survival),
        apparent_support: mean(|m| m.apparent_support),
        opinion_clusters: slice.last().map(|m| m.opinion_clusters).unwrap_or(0),
        expression_entropy: mean(|m| m.expression_entropy),
    }
}

/// LLM モードの永続プロンプトキャッシュの親ディレクトリを先に作る．
///
/// cold→warm 再生のため `--cache-path` のファイルをプロセスをまたいで保持する．
fn ensure_cache_parent(cfg: &Config) {
    if cfg.decision_mode != DecisionMode::Llm {
        return;
    }
    if let Some(path) = cfg.cache_path.as_deref() {
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: RunArgs) {
    // シードを実体化してから記録する．--seed 省略時にシミュレーション側で
    // rand::random に落とすと，実際に使われたシードがどこにも残らない．
    let seed = args.seed.unwrap_or_else(rand::random::<u64>);
    let mut cfg = args.to_config(seed);
    ensure_cache_parent(&cfg);

    // LLM クライアントは `Run::start` の前に組む — モデル名と endpoint を知っているのは
    // クライアントを組んだ側だけで，`run.json` の `llm` ブロックはその 2 つを要る．
    let prepared: Option<PreparedLlm> = match cfg.decision_mode {
        DecisionMode::Rule => None,
        DecisionMode::Llm => Some(prepare_llm(&cfg)),
    };

    let parameters = cfg.to_parameters(seed);
    let mut options = RunOptions::new(EXPERIMENT, "run")
        .repo_id(REPO_ID)
        .domain(DOMAIN)
        .results_root(&args.output_dir)
        .parameters(&parameters)
        .expect("runvault: parameters の組み立てに失敗")
        .seed_pointers(["/seed"])
        .master_seed(seed)
        .replication(record::replication());
    if let Some(p) = &prepared {
        let id = p.identity();
        options = options.llm(record::llm_block(&id.model, &id.endpoint, id.temperature));
    }
    let mut rv = Run::start(options).expect("runvault: run の開始に失敗");

    // run ディレクトリが出力先そのものになる．意見の軌跡は artifacts/ の下へ．
    cfg.output_dir = rv.dir().join("artifacts").to_string_lossy().into_owned();

    println!("=== Noelle-Neumann 「沈黙の螺旋」 再現実験 ===");
    println!(
        "n: {} | q: {} | 網: {} (k={}, β={}) | η_m: {} | α: {} | β_π: {} | β_f: {}",
        cfg.n,
        cfg.true_support,
        cfg.network_model.label(),
        cfg.network_k,
        cfg.network_beta,
        cfg.eta_m,
        cfg.alpha,
        cfg.beta_pi,
        cfg.beta_fear,
    );
    println!(
        "hardcore: {} | t_max: {} | mode: {} | seed: {}",
        cfg.hardcore_frac,
        cfg.t_max,
        cfg.decision_mode.label(),
        seed,
    );
    println!("出力先: {}", rv.dir().display());
    println!("-------------------------------------------");

    let (result, usage): (SimulationResult, Option<LlmUsage>) = match prepared {
        None => (run(&cfg), None),
        Some(p) => {
            let (result, usage) = run_prepared(&cfg, p);
            (result, Some(usage))
        }
    };
    save_opinions(&result.snapshots, &cfg.output_dir);

    record::log_simulation(&mut rv, &result);
    if let Some(usage) = &usage {
        record::log_llm_usage(&mut rv, usage);
        println!(
            "LLM 呼び出し: {} | cache-hit: {} ({:.1}%)",
            usage.calls,
            usage.cache_hits,
            usage.cache_hit_rate * 100.0,
        );
    }

    let ss = steady_state(&result);
    // run は全 tick を観測して metrics.csv に残しているので，観測時刻も全 tick．
    let observed: Vec<u64> = result.metrics_history.iter().map(|m| m.t as u64).collect();
    record::log_terminal(&mut rv, "run", seed, cfg.t_max, observed, &result, &ss);

    let final_world_metrics = result.metrics_history.last().unwrap();
    println!(
        "収束: {} | tick: {}",
        if result.converged { "Yes" } else { "No" },
        result.final_tick
    );
    println!("--- 定常状態 (後半平均) ---");
    println!("voice_volume          : {:.4}", ss.voice_volume);
    println!(
        "majority_voice_ratio  : {:.4} (log-OR; H1 アンカー >0.8)",
        ss.majority_voice_ratio
    );
    println!(
        "perceived_minus_actual: {:.4} (H3)",
        ss.perceived_minus_actual
    );
    println!(
        "future_assessment_gap : {:.4} (H5 アンカー >0.4)",
        ss.future_assessment_gap
    );
    println!("hardcore_survival     : {:.4}", ss.hardcore_survival);
    println!(
        "apparent_support q̂    : {:.4}",
        final_world_metrics.apparent_support
    );
    println!("opinion_clusters      : {}", ss.opinion_clusters);

    let dir = rv.finish().expect("runvault: run の完了に失敗");
    println!("意見軌跡   → {}/artifacts/opinions.csv", dir.display());
    println!("メトリクス → {}/metrics.csv", dir.display());
    println!("終端       → {}/events.jsonl", dir.display());
    println!("設定       → {}/config.json", dir.display());
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

fn cmd_sweep(args: SweepArgs) {
    let eta_ms = parse_f64_list(&args.eta_m_values);
    let betas = parse_f64_list(&args.network_beta_values);
    let alphas = parse_f64_list(&args.alpha_values);
    let ks = parse_usize_list(&args.network_k_values);
    let qs = parse_f64_list(&args.true_support_values);

    let n_conditions = eta_ms.len() * betas.len() * alphas.len() * ks.len() * qs.len();
    let n_total = n_conditions * args.runs;

    let sweep_parameters = SweepParameters {
        eta_m_values: eta_ms.clone(),
        network_beta_values: betas.clone(),
        alpha_values: alphas.clone(),
        network_k_values: ks.clone(),
        true_support_values: qs.clone(),
        runs: args.runs,
        n: args.n,
        hardcore_frac: args.hardcore_frac,
        t_max: args.t_max,
        seed: args.seed,
    };

    // 親 run: 走査グリッドの定義そのものを parameters に持つ．個別条件の指標は書かない．
    // 親は 1 本のシミュレーションではないので master_seed を名乗らず，base seed は
    // /parameters.seed と seed_pointers 経由で execution_hash に残る．
    // sweep_id は runvault が親の run_slug で埋める．
    let parent = Run::start(
        RunOptions::new(EXPERIMENT, "sweep")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&args.output_dir)
            .parameters(&sweep_parameters)
            .expect("runvault: sweep の parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .sweep_parent()
            .replication(record::replication()),
    )
    .expect("runvault: sweep 親 run の開始に失敗");

    let sweep_id = parent
        .sweep_id()
        .expect("runvault: sweep 親に sweep_id がありません")
        .to_string();
    let parent_run_uid = parent.run_uid().to_string();

    println!("=== 「沈黙の螺旋」 パラメータスイープ ===");
    println!(
        "η_m: {} | β: {} | α: {} | k: {} | q: {} | runs: {} | 条件: {} | 合計: {} 実行",
        eta_ms.len(),
        betas.len(),
        alphas.len(),
        ks.len(),
        qs.len(),
        args.runs,
        n_conditions,
        n_total
    );
    println!("シード (base): {}", args.seed);
    println!("出力先: {}", parent.dir().display());
    println!("---------------------------------------------------");

    let mut done = 0usize;

    for &eta_m in &eta_ms {
        for &network_beta in &betas {
            for &alpha in &alphas {
                for &network_k in &ks {
                    for &q in &qs {
                        let params = SweepPointParameters {
                            n: args.n,
                            true_support: q,
                            network_k,
                            network_beta,
                            eta_m,
                            alpha,
                            hardcore_frac: args.hardcore_frac,
                            t_max: args.t_max,
                            runs: args.runs,
                            seed: args.seed,
                        };

                        // 子は「その格子点の試行群」そのもの．master_seed は親と同じ
                        // base で，条件が違えば config_hash が違うので run としては
                        // 別物になる．同じ条件の繰り返しは無いので replicate_index は 0．
                        let mut child = Run::start(
                            RunOptions::new(EXPERIMENT, "sweep-point")
                                .repo_id(REPO_ID)
                                .domain(DOMAIN)
                                .results_root(&args.output_dir)
                                .parameters(&params)
                                .expect("runvault: 子 run の parameters の組み立てに失敗")
                                .seed_pointers(["/seed"])
                                .master_seed(args.seed)
                                .replicate_index(0)
                                .lineage(Lineage {
                                    sweep_id: Some(sweep_id.clone()),
                                    parent_run_uid: Some(parent_run_uid.clone()),
                                    ..Default::default()
                                })
                                .replication(record::replication()),
                        )
                        .expect("runvault: 子 run の開始に失敗");

                        let mut trials: Vec<TrialOutcome> = Vec::with_capacity(args.runs);
                        for run_idx in 0..args.runs {
                            // 各 (条件, run) に独立なシードを派生させる (explicit identity)．
                            let seed = record::trial_seed(
                                args.seed,
                                eta_m,
                                network_beta,
                                alpha,
                                network_k,
                                q,
                                run_idx,
                            );
                            let cfg = Config {
                                n: args.n,
                                true_support: q,
                                network_k,
                                network_beta,
                                eta_m,
                                alpha,
                                hardcore_frac: args.hardcore_frac,
                                t_max: args.t_max,
                                seed: Some(seed),
                                output_dir: String::new(),
                                ..Config::default()
                            };
                            let result = run(&cfg);
                            let steady = steady_state(&result);
                            // sweep が見るのは各試行の定常状態だけなので，観測時刻も
                            // その最終 tick 1 点．
                            record::log_terminal(
                                &mut child,
                                &format!("trial-{run_idx}"),
                                seed,
                                args.t_max,
                                [result.final_tick as u64],
                                &result,
                                &steady,
                            );
                            trials.push(TrialOutcome {
                                converged: result.converged,
                                final_tick: result.final_tick,
                                steady,
                            });
                            done += 1;
                        }
                        record::log_condition_summary(&mut child, &trials);

                        let mean_log_or = trials
                            .iter()
                            .map(|t| t.steady.majority_voice_ratio)
                            .sum::<f64>()
                            / trials.len() as f64;
                        println!(
                            "[{}/{}] η_m={} β={} α={} k={} q={} 完了 ({} 試行) \
                             → mean log-OR={:.3}",
                            done,
                            n_total,
                            eta_m,
                            network_beta,
                            alpha,
                            network_k,
                            q,
                            args.runs,
                            mean_log_or,
                        );

                        child.finish().expect("runvault: 子 run の完了に失敗");
                    }
                }
            }
        }
    }

    let dir = parent
        .finish()
        .expect("runvault: sweep 親 run の完了に失敗");
    println!("===================================================");
    println!("スイープ完了: {} 実行", n_total);
    println!("スイープ定義 → {}/config.json", dir.display());
    println!("各条件の試行は子 run (subcommand=sweep-point) の events.jsonl にあります");
}

// ---------------------------------------------------------------------------
// reproduce
// ---------------------------------------------------------------------------

/// ハードコア境界シナリオの hardcore 比率 (baseline は 0.05)．
const BOUNDARY_HARDCORE_FRAC: f64 = 0.25;

fn cmd_reproduce(args: ReproduceArgs) {
    // 社会主義到来シナリオ (q=0.37): 勝/負陣営の発言意欲非対称 (Table 2)．
    let mut cfg = Config {
        n: args.n,
        true_support: 0.37,
        eta_m: 0.5,
        alpha: 0.7,
        hardcore_frac: 0.05,
        t_max: args.t_max,
        seed: Some(args.seed),
        output_dir: String::new(),
        ..Config::default()
    };

    let parameters = ReproduceParameters {
        baseline: cfg.to_parameters(args.seed),
        boundary_hardcore_frac: BOUNDARY_HARDCORE_FRAC,
    };
    let mut rv = Run::start(
        RunOptions::new(EXPERIMENT, "reproduce")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&args.output_dir)
            .parameters(&parameters)
            .expect("runvault: parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .master_seed(args.seed)
            .replication(record::replication()),
    )
    .expect("runvault: run の開始に失敗");

    cfg.output_dir = rv.dir().join("artifacts").to_string_lossy().into_owned();

    println!("=== 「沈黙の螺旋」 Table 1--5 アンカー再現 ===");
    println!("出力先: {}", rv.dir().display());

    let result = run(&cfg);
    let ss = steady_state(&result);
    save_opinions(&result.snapshots, &cfg.output_dir);
    record::log_simulation(&mut rv, &result);
    let observed: Vec<u64> = result.metrics_history.iter().map(|m| m.t as u64).collect();
    record::log_terminal(
        &mut rv, "baseline", args.seed, cfg.t_max, observed, &result, &ss,
    );

    // 陣営別 voice / pi は最終 tick の世界状態が必要なので，専用にもう一度実行して
    // world を取得する (ドライバ本体と同一の配線・seed なので軌跡は同一)．
    let world = final_world(&cfg);
    let (maj_voice, min_voice) = voice_by_camp(&world);
    let (pi_pro, pi_con) = pi_now_by_camp(&world);

    // ハードコア境界シナリオ (hardcore_frac=0.25)．baseline との比較は 1 回の実行の
    // 中で定義されているので，子 run には分けない．
    let cfg_hc = Config {
        hardcore_frac: BOUNDARY_HARDCORE_FRAC,
        ..cfg.clone()
    };
    let world_hc = final_world(&cfg_hc);
    let hc_survival = noelleneumann_spiral_simulation::metrics::hardcore_survival(&world_hc);

    // 帯はどれも本再現が置いたもので，論文の報告値ではない (論文値は
    // record::log_paper_references が出典付きで reference.csv に書く)．
    let anchors = vec![
        Anchor::band("steady_voice_volume", ss.voice_volume, 0.30, 0.42),
        Anchor::band("final_majority_voice", maj_voice, 0.45, 0.65),
        Anchor::band("final_minority_voice", min_voice, 0.15, 0.40),
        Anchor::at_least("steady_majority_voice_ratio", ss.majority_voice_ratio, 0.8),
        Anchor::band("final_pi_now_pro", pi_pro, 0.40, 0.55),
        Anchor::band("final_pi_now_con", pi_con, 0.40, 0.60),
        Anchor::at_least(
            "steady_future_assessment_gap",
            ss.future_assessment_gap,
            0.4,
        ),
        Anchor::at_least("boundary_hardcore_survival", hc_survival, 0.7),
    ];
    record::log_anchor_observations(&mut rv, &anchors);
    record::log_anchors(&mut rv, &anchors);
    record::log_paper_references(&mut rv);

    println!("---------------------------------------------------");
    for a in &anchors {
        let hi = match a.target_hi {
            Some(hi) => format!("{:.2}", hi),
            None => "∞".to_string(),
        };
        println!(
            "[{}] {:<30} obs={:.4} target=[{:.2},{}]",
            if a.pass { "PASS" } else { "OFF " },
            a.indicator,
            a.observed,
            a.target_lo,
            hi,
        );
    }
    let n_pass = anchors.iter().filter(|a| a.pass).count();
    println!("---------------------------------------------------");
    println!("{}/{} アンカーが in-band", n_pass, anchors.len());

    let dir = rv.finish().expect("runvault: run の完了に失敗");
    println!("帯照合   → {}/events.jsonl", dir.display());
    println!("論文値   → {}/reference.csv", dir.display());
    println!("観測量   → {}/metrics.csv", dir.display());
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();
    if let Some(host) = cli.ollama_host.as_deref() {
        std::env::set_var("OLLAMA_HOST", host);
    }
    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::Sweep(args) => cmd_sweep(args),
        Commands::Reproduce(args) => cmd_reproduce(args),
    }
}
