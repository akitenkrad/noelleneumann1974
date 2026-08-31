//! オフライン (LLM 不要) スモーク: scripted mock で LLM 版パイプライン全体を駆動し，
//! 本番 `run` と同じ経路で runvault の run ディレクトリへ書き出す (live LLM 非依存)．
//!
//! Usage:
//!     cargo run --release --features llm --example mock_smoke -- results
//!
//! 引数は runvault の results ルート (run ディレクトリの名前は runvault が決める)．
//! feature `llm` 無しでビルドした場合は何もせずメッセージのみ出す．

#[cfg(feature = "llm")]
fn main() {
    use noelleneumann_spiral_simulation::config::{Config, DecisionMode};
    use noelleneumann_spiral_simulation::record::{self, DOMAIN, EXPERIMENT, REPO_ID};
    use noelleneumann_spiral_simulation::simulation::{run_prepared, save_opinions, PreparedLlm};
    use runvault::{Run, RunOptions};
    use socsim_llm::mock::ScriptedClient;
    use socsim_llm::PromptCache;

    use noelleneumann_spiral_simulation::llm::wrap_client;

    let results_root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results".to_string());

    let seed = 42u64;
    let mut cfg = Config {
        n: 200,
        true_support: 0.37,
        t_max: 30,
        seed: Some(seed),
        decision_mode: DecisionMode::Llm,
        output_dir: String::new(),
        ..Config::default()
    };

    let backend = ScriptedClient::new("mock-spiral", |prompt: &str| {
        let favourable = prompt.contains("the clear majority") || prompt.contains("roughly half");
        if favourable {
            "Reflecting, I feel safe.\nSPEAK".to_string()
        } else {
            "I would feel isolated.\nSILENT".to_string()
        }
    });
    // mock は in-memory キャッシュで回す (永続キャッシュは live 経路のもの)．
    let prepared = PreparedLlm::new(
        wrap_client(backend, PromptCache::in_memory()),
        cfg.llm_temperature,
    );
    let identity = prepared.identity().clone();

    let parameters = cfg.to_parameters(seed);
    let mut rv = Run::start(
        RunOptions::new(EXPERIMENT, "mock-smoke")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&results_root)
            .parameters(&parameters)
            .expect("runvault: parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .master_seed(seed)
            .llm(record::llm_block(
                &identity.model,
                &identity.endpoint,
                identity.temperature,
            ))
            .replication(record::replication()),
    )
    .expect("runvault: run の開始に失敗");

    cfg.output_dir = rv.dir().join("artifacts").to_string_lossy().into_owned();

    let (result, usage) = run_prepared(&cfg, prepared);
    save_opinions(&result.snapshots, &cfg.output_dir);
    record::log_simulation(&mut rv, &result);
    record::log_llm_usage(&mut rv, &usage);

    println!("final_tick: {}", result.final_tick);
    let last = result.metrics_history.last().unwrap();
    println!("voice_volume: {:.4}", last.voice_volume);
    println!(
        "LLM 呼び出し: {} | cache-hit: {} ({:.1}%)",
        usage.calls,
        usage.cache_hits,
        usage.cache_hit_rate * 100.0
    );

    let dir = rv.finish().expect("runvault: run の完了に失敗");
    println!("mock smoke wrote: {}", dir.display());
}

#[cfg(not(feature = "llm"))]
fn main() {
    eprintln!(
        "mock_smoke は --features llm でビルドした場合のみ動作します \
         (cargo run --release --features llm --example mock_smoke -- results)"
    );
}
