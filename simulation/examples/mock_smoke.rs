//! オフライン (LLM 不要) スモーク: scripted mock で LLM 版パイプライン全体を駆動し，
//! 本番 `run` と同じ writer で results を書き出す (live LLM 非依存)．
//!
//! Usage:
//!     cargo run --release --features llm --example mock_smoke -- results
//!
//! feature `llm` 無しでビルドした場合は何もせずメッセージのみ出す．

#[cfg(feature = "llm")]
fn main() {
    use noelleneumann_spiral_simulation::config::{Config, DecisionMode};
    use noelleneumann_spiral_simulation::llm::{run_with_client, wrap_client};
    use noelleneumann_spiral_simulation::simulation::{
        ensure_output_dir, save_metrics, save_opinions, write_json_file,
    };
    use socsim_llm::mock::ScriptedClient;
    use socsim_llm::PromptCache;
    use socsim_results::{timestamp, write_json};

    let base = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results".to_string());
    let ts = timestamp();
    let output_dir = format!("{}/{}_mock", base, ts);

    let cfg = Config {
        n: 200,
        true_support: 0.37,
        t_max: 30,
        seed: Some(42),
        decision_mode: DecisionMode::Llm,
        output_dir: output_dir.clone(),
        ..Config::default()
    };
    ensure_output_dir(&output_dir);

    let backend = ScriptedClient::new("mock-spiral", |prompt: &str| {
        let favourable = prompt.contains("the clear majority") || prompt.contains("roughly half");
        if favourable {
            "Reflecting, I feel safe.\nSPEAK".to_string()
        } else {
            "I would feel isolated.\nSILENT".to_string()
        }
    });
    let client = wrap_client(backend, PromptCache::in_memory());

    let result = run_with_client(&cfg, client);
    save_metrics(&result.metrics_history, &output_dir);
    save_opinions(&result.snapshots, &output_dir);
    write_json(
        &cfg.to_run_config_json(),
        format!("{}/config.json", output_dir),
    )
    .expect("config.json の書き込みに失敗");
    let meta = serde_json::json!({
        "decision_mode": "llm",
        "backend": "mock::ScriptedClient",
        "temperature": 0.0,
        "seed": cfg.seed,
    });
    write_json_file(&meta, &format!("{}/llm_meta.json", output_dir));

    println!("mock smoke wrote: {output_dir}");
    println!("final_tick: {}", result.final_tick);
    let last = result.metrics_history.last().unwrap();
    println!("voice_volume: {:.4}", last.voice_volume);
}

#[cfg(not(feature = "llm"))]
fn main() {
    eprintln!(
        "mock_smoke は --features llm でビルドした場合のみ動作します \
         (cargo run --release --features llm --example mock_smoke -- results)"
    );
}
