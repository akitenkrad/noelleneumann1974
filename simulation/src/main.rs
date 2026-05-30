//! Noelle-Neumann (1974) "The Spiral of Silence" — 再現実験の CLI エントリポイント．
//!
//! `run`       : 単一条件で「沈黙の螺旋」を実行する (rule / llm)．
//! `sweep`     : η_m / network_beta / α 等を走査して境界条件を集計する．
//! `reproduce` : §5 のアレンスバッハ調査 Table 1--5 アンカーと baseline 出力を照合する．

use clap::{Parser, Subcommand};
use socsim_results::{refresh_latest_symlink, timestamp, write_csv, write_json};

use noelleneumann_spiral_simulation::config::{
    parse_decision_mode, parse_network_model, Config, DecisionMode,
};
use noelleneumann_spiral_simulation::metrics::{pi_now_by_camp, voice_by_camp, Metrics};
use noelleneumann_spiral_simulation::simulation::{
    ensure_output_dir, run, run_dispatch_with_meta, save_metrics, save_opinions, write_json_file,
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
    /// 乱数シード．
    #[arg(long)]
    seed: Option<u64>,
    /// 結果出力ディレクトリ．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

impl RunArgs {
    fn to_config(&self, output_dir: String) -> Config {
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
            seed: self.seed,
            output_dir,
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
    /// 出力ベースディレクトリ．
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
    /// 出力ベースディレクトリ．
    #[arg(long, default_value = "results")]
    output_dir: String,
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

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: RunArgs) {
    let ts = timestamp();
    let output_dir = format!("{}/{}", args.output_dir, ts);
    let cfg = args.to_config(output_dir.clone());
    ensure_output_dir(&cfg.output_dir);

    // LLM モードでは永続プロンプトキャッシュの親ディレクトリを先に作る
    // (cold→warm 再生のため `--cache-path` のファイルを跨プロセスで保持する)．
    if cfg.decision_mode == DecisionMode::Llm {
        if let Some(path) = cfg.cache_path.as_deref() {
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }

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
        "hardcore: {} | t_max: {} | mode: {} | seed: {:?}",
        cfg.hardcore_frac,
        cfg.t_max,
        cfg.decision_mode.label(),
        cfg.seed
    );
    println!("出力先: {}", cfg.output_dir);
    println!("-------------------------------------------");

    let (result, llm_meta) = run_dispatch_with_meta(&cfg);
    save_metrics(&result.metrics_history, &cfg.output_dir);
    save_opinions(&result.snapshots, &cfg.output_dir);

    let cfg_path = format!("{}/config.json", cfg.output_dir);
    write_json(&cfg.to_run_config_json(), &cfg_path).expect("config.json の書き込みに失敗");

    // LLM モードでは `MetadataCollector` 由来の実 cache 統計
    // (model / endpoint / temperature / seed / calls / cache_hits / cache_hit_rate)
    // を llm_meta.json に書く (placeholder ではない)．rule モードは LLM 非呼び出しなので書かない．
    if let Some(meta) = &llm_meta {
        write_json_file(meta, &format!("{}/llm_meta.json", cfg.output_dir));
        let calls = meta.get("calls").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_hits = meta.get("cache_hits").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_hit_rate = meta
            .get("cache_hit_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let model = meta.get("model").and_then(|v| v.as_str()).unwrap_or("?");
        println!(
            "LLM 呼び出し: {} | cache-hit: {} ({:.1}%) | model: {}",
            calls,
            cache_hits,
            cache_hit_rate * 100.0,
            model,
        );
    }

    let _ = refresh_latest_symlink(&args.output_dir, &ts);

    let ss = steady_state(&result);
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
    println!("意見軌跡 → {}/opinions.csv", cfg.output_dir);
    println!("メトリクス → {}/metrics.csv", cfg.output_dir);
    println!("設定       → {}/config.json", cfg.output_dir);
    if llm_meta.is_some() {
        println!("LLM メタ   → {}/llm_meta.json", cfg.output_dir);
    }
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct SweepRow {
    eta_m: f64,
    network_beta: f64,
    alpha: f64,
    network_k: usize,
    true_support: f64,
    run: usize,
    seed: u64,
    voice_volume: f64,
    majority_voice_ratio: f64,
    perceived_minus_actual: f64,
    future_assessment_gap: f64,
    hardcore_survival: f64,
    apparent_support: f64,
    converged: bool,
    final_tick: usize,
}

#[derive(serde::Serialize)]
struct SweepConfigJson {
    command: &'static str,
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

fn cmd_sweep(args: SweepArgs) {
    let eta_ms = parse_f64_list(&args.eta_m_values);
    let betas = parse_f64_list(&args.network_beta_values);
    let alphas = parse_f64_list(&args.alpha_values);
    let ks = parse_usize_list(&args.network_k_values);
    let qs = parse_f64_list(&args.true_support_values);

    let ts = timestamp();
    let sweep_dir = format!("{}/{}_sweep", args.output_dir, ts);
    std::fs::create_dir_all(&sweep_dir).expect("sweep ディレクトリの作成に失敗");

    let n_total = eta_ms.len() * betas.len() * alphas.len() * ks.len() * qs.len() * args.runs;
    println!("=== 「沈黙の螺旋」 パラメータスイープ ===");
    println!(
        "η_m: {} | β: {} | α: {} | k: {} | q: {} | runs: {} | 合計: {} 実行",
        eta_ms.len(),
        betas.len(),
        alphas.len(),
        ks.len(),
        qs.len(),
        args.runs,
        n_total
    );
    println!("出力先: {}", sweep_dir);
    println!("---------------------------------------------------");

    let mut rows: Vec<SweepRow> = Vec::with_capacity(n_total);
    let mut done = 0usize;

    for &eta_m in &eta_ms {
        for &network_beta in &betas {
            for &alpha in &alphas {
                for &network_k in &ks {
                    for &q in &qs {
                        for run_idx in 0..args.runs {
                            let seed = socsim_core::derive_seed(
                                args.seed,
                                &[
                                    eta_m.to_bits(),
                                    network_beta.to_bits(),
                                    alpha.to_bits(),
                                    network_k as u64,
                                    q.to_bits(),
                                    run_idx as u64,
                                ],
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
                                output_dir: sweep_dir.clone(),
                                ..Config::default()
                            };
                            let result = run(&cfg);
                            let ss = steady_state(&result);
                            rows.push(SweepRow {
                                eta_m,
                                network_beta,
                                alpha,
                                network_k,
                                true_support: q,
                                run: run_idx,
                                seed,
                                voice_volume: ss.voice_volume,
                                majority_voice_ratio: ss.majority_voice_ratio,
                                perceived_minus_actual: ss.perceived_minus_actual,
                                future_assessment_gap: ss.future_assessment_gap,
                                hardcore_survival: ss.hardcore_survival,
                                apparent_support: ss.apparent_support,
                                converged: result.converged,
                                final_tick: result.final_tick,
                            });
                            done += 1;
                        }
                    }
                }
            }
        }
        println!("[{}/{}] η_m={} まで完了", done, n_total, eta_m);
    }

    let summary_path = format!("{}/sweep_summary.csv", sweep_dir);
    write_csv(&rows, &summary_path).expect("sweep_summary.csv の書き込みに失敗");

    let cfg_json = SweepConfigJson {
        command: "sweep",
        eta_m_values: eta_ms,
        network_beta_values: betas,
        alpha_values: alphas,
        network_k_values: ks,
        true_support_values: qs,
        runs: args.runs,
        n: args.n,
        hardcore_frac: args.hardcore_frac,
        t_max: args.t_max,
        seed: args.seed,
    };
    let cfg_path = format!("{}/sweep_config.json", sweep_dir);
    write_json(&cfg_json, &cfg_path).expect("sweep_config.json の書き込みに失敗");

    let _ = refresh_latest_symlink(&args.output_dir, &format!("{}_sweep", ts));

    println!("===================================================");
    println!("スイープ完了: {} 実行", n_total);
    println!("サマリ → {}/sweep_summary.csv", sweep_dir);
}

// ---------------------------------------------------------------------------
// reproduce
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct AnchorResult {
    name: String,
    paper_value: String,
    observed: f64,
    target_lo: f64,
    target_hi: f64,
    pass: bool,
}

fn cmd_reproduce(args: ReproduceArgs) {
    let ts = timestamp();
    let out_dir = format!("{}/{}_reproduce", args.output_dir, ts);
    ensure_output_dir(&out_dir);

    println!("=== 「沈黙の螺旋」 Table 1--5 アンカー再現 ===");
    println!("出力先: {}", out_dir);

    // 社会主義到来シナリオ (q=0.37): 勝/負陣営の発言意欲非対称 (Table 2)．
    let cfg = Config {
        n: args.n,
        true_support: 0.37,
        eta_m: 0.5,
        alpha: 0.7,
        hardcore_frac: 0.05,
        t_max: args.t_max,
        seed: Some(args.seed),
        output_dir: out_dir.clone(),
        ..Config::default()
    };
    let result = run(&cfg);
    let final_world = &result;
    let ss = steady_state(final_world);

    // 最終世界での陣営別指標を取り直すため，最終 snapshot を使う代わりに run を再実行
    // して world を取得 (run は world を返さないので metrics 履歴の定常値で代替する)．
    // 陣営別 voice / pi は最終 tick の世界状態が必要なので，専用にもう一度実行する．
    let world = noelleneumann_spiral_simulation::simulation::final_world(&cfg);
    let (maj_voice, min_voice) = voice_by_camp(&world);
    let (pi_pro, pi_con) = pi_now_by_camp(&world);

    let mut anchors: Vec<AnchorResult> = Vec::new();
    let mut push = |name: &str, paper: &str, obs: f64, lo: f64, hi: f64| {
        anchors.push(AnchorResult {
            name: name.to_string(),
            paper_value: paper.to_string(),
            observed: obs,
            target_lo: lo,
            target_hi: hi,
            pass: obs >= lo && obs <= hi,
        });
    };

    // Table 1: 全体発言意欲 36%．
    push(
        "voice_volume (Table 1: 36%)",
        "0.36",
        ss.voice_volume,
        0.30,
        0.42,
    );
    // Table 2: 勝ち組 53% / 負け組 28%．
    push(
        "majority_voice (Table 2: 53%)",
        "0.53",
        maj_voice,
        0.45,
        0.65,
    );
    push(
        "minority_voice (Table 2: 28%)",
        "0.28",
        min_voice,
        0.15,
        0.40,
    );
    // H1 中核アンカー: log-OR > 0.8．
    push(
        "majority_voice_ratio (H1: log-OR>0.8)",
        ">0.8",
        ss.majority_voice_ratio,
        0.8,
        f64::INFINITY,
    );
    // Table 4: pi_now 賛成派 ~0.49 / 反対派 ~0.43 (1-0.57)．
    push("pi_now pro (Table 4: ~0.49)", "0.49", pi_pro, 0.40, 0.55);
    push("pi_now con (Table 4: ~0.43)", "0.43", pi_con, 0.40, 0.60);
    // H5 アンカー: future_assessment_gap > 0.4 (Table 5: 70% vs 23%)．
    push(
        "future_assessment_gap (H5: >0.4)",
        ">0.4",
        ss.future_assessment_gap,
        0.4,
        f64::INFINITY,
    );

    // ハードコア境界シナリオ (hardcore_frac=0.25)．
    let cfg_hc = Config {
        hardcore_frac: 0.25,
        ..cfg.clone()
    };
    let world_hc = noelleneumann_spiral_simulation::simulation::final_world(&cfg_hc);
    let hc_survival = noelleneumann_spiral_simulation::metrics::hardcore_survival(&world_hc);
    push(
        "hardcore_survival (hardcore_frac=0.25: >0.7)",
        ">0.7",
        hc_survival,
        0.7,
        f64::INFINITY,
    );

    println!("---------------------------------------------------");
    for a in &anchors {
        let hi = if a.target_hi.is_infinite() {
            "∞".to_string()
        } else {
            format!("{:.2}", a.target_hi)
        };
        println!(
            "[{}] {:<42} obs={:.4} target=[{:.2},{}] paper={}",
            if a.pass { "PASS" } else { "OFF " },
            a.name,
            a.observed,
            a.target_lo,
            hi,
            a.paper_value,
        );
    }
    let n_pass = anchors.iter().filter(|a| a.pass).count();
    println!("---------------------------------------------------");
    println!("{}/{} アンカーが in-band", n_pass, anchors.len());

    save_metrics(&result.metrics_history, &out_dir);
    save_opinions(&result.snapshots, &out_dir);
    let report = serde_json::json!({ "anchors": anchors });
    write_json_file(&report, &format!("{}/reproduction_report.json", out_dir));
    let _ = refresh_latest_symlink(&args.output_dir, &format!("{}_reproduce", ts));
    println!("レポート → {}/reproduction_report.json", out_dir);
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::Sweep(args) => cmd_sweep(args),
        Commands::Reproduce(args) => cmd_reproduce(args),
    }
}
