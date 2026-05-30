//! LLM 内省版発言決定の **mock (scripted) smoke テスト** (feature `llm` 時のみ)．
//!
//! live LLM (Ollama / OpenAI) には一切依存しない．`socsim_llm::mock::ScriptedClient`
//! でプロンプト内容 (知覚気候の語) から SPEAK / SILENT を決定論的に返し，パイプライン
//! 全体 (LlmOracle → voice_decision → silence_spiral → metrics) が動くこと，および
//! プロンプトキャッシュで擬似決定論になることを検証する．
#![cfg(feature = "llm")]

use noelleneumann_spiral_simulation::config::{Config, DecisionMode};
use noelleneumann_spiral_simulation::llm::{run_with_client, wrap_client};
use socsim_llm::mock::ScriptedClient;
use socsim_llm::PromptCache;

/// 「自分の側が多数」と知覚するプロンプトには SPEAK，劣勢なら SILENT を返す mock．
fn scripted() -> ScriptedClient {
    ScriptedClient::new("mock-spiral", |prompt: &str| {
        // voice_introspection_prompt は現在/未来気候を語で埋め込む．
        // 「clear majority」や「roughly half」を含めば発言寄り，劣勢語なら沈黙寄り．
        let favourable = prompt.contains("the clear majority") || prompt.contains("roughly half");
        if favourable {
            "Reflecting, I feel safe.\nSPEAK".to_string()
        } else {
            "I would feel isolated.\nSILENT".to_string()
        }
    })
}

#[test]
fn llm_mock_pipeline_runs() {
    let cfg = Config {
        n: 120,
        true_support: 0.4,
        t_max: 20,
        seed: Some(42),
        decision_mode: DecisionMode::Llm,
        ..Config::default()
    };
    let client = wrap_client(scripted(), PromptCache::in_memory());
    let result = run_with_client(&cfg, client);
    assert_eq!(result.snapshots[0].len(), 120);
    assert!(result.final_tick > 0, "少なくとも 1 tick 進むべき");
    // 全体として一部は発言・一部は沈黙する (極端な全沈黙/全発言ではない)．
    let last = result.metrics_history.last().unwrap();
    assert!(
        last.voice_volume >= 0.0 && last.voice_volume <= 1.0,
        "voice_volume が [0,1] にある"
    );
}

#[test]
fn llm_mock_is_deterministic_with_cache() {
    let cfg = Config {
        n: 80,
        t_max: 15,
        seed: Some(7),
        decision_mode: DecisionMode::Llm,
        ..Config::default()
    };
    let a = run_with_client(&cfg, wrap_client(scripted(), PromptCache::in_memory()));
    let b = run_with_client(&cfg, wrap_client(scripted(), PromptCache::in_memory()));
    assert_eq!(a.final_tick, b.final_tick);
    let la = a.snapshots.last().unwrap();
    let lb = b.snapshots.last().unwrap();
    for (x, y) in la.iter().zip(lb.iter()) {
        assert_eq!(x.1, y.1, "同一 scripted mock + seed で公的表出が一致すべき");
    }
}
