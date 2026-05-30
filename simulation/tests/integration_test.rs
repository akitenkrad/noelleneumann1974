//! Noelle-Neumann (1974)「沈黙の螺旋」再現実装の統合テスト．
//!
//! ライブラリ公開 API に対して，
//! ・ルールモードのビット決定論性 (同一シードで公的表出列がバイト等価)
//! ・沈黙の螺旋の中核非対称 (多数派 log-OR > 0，知覚-真値乖離が生じる)
//! ・H5 未来評価ギャップが正
//! ・網モデル切替が全て動く
//! を検証する．LLM 版は `--features llm` の下で別途 mock smoke テストが走る (live LLM
//! 非依存; 本ファイルは feature 非依存のルールモードのみ)．

use noelleneumann_spiral_simulation::config::{Config, NetworkModel};
use noelleneumann_spiral_simulation::metrics::{
    future_assessment_gap, hardcore_survival, majority_voice_ratio,
};
use noelleneumann_spiral_simulation::simulation::{final_world, run};

fn base_config() -> Config {
    Config {
        n: 600,
        true_support: 0.37,
        t_max: 60,
        seed: Some(42),
        ..Config::default()
    }
}

// --------------------------------------------------------------------------- //
// ルールモードのビット決定論性
// --------------------------------------------------------------------------- //

#[test]
fn rule_mode_is_bit_deterministic() {
    let a = run(&base_config());
    let b = run(&base_config());
    assert_eq!(a.final_tick, b.final_tick, "終了 tick が一致すべき");
    assert_eq!(a.snapshots.len(), b.snapshots.len());
    // 全 tick・全エージェントの (b, e_code, pi_now, pi_fut) がバイト等価．
    for (sa, sb) in a.snapshots.iter().zip(b.snapshots.iter()) {
        assert_eq!(sa.len(), sb.len());
        for (x, y) in sa.iter().zip(sb.iter()) {
            assert_eq!(x.0.to_bits(), y.0.to_bits(), "b がビット等価でない");
            assert_eq!(x.1, y.1, "公的表出がビット等価でない");
            assert_eq!(x.2.to_bits(), y.2.to_bits(), "pi_now がビット等価でない");
            assert_eq!(x.3.to_bits(), y.3.to_bits(), "pi_fut がビット等価でない");
        }
    }
}

// --------------------------------------------------------------------------- //
// H1: 多数派の発言意欲が少数派を上回る (沈黙の螺旋の中核非対称)
// --------------------------------------------------------------------------- //

#[test]
fn majority_voices_more_than_minority() {
    let world = final_world(&base_config());
    let log_or = majority_voice_ratio(&world);
    assert!(
        log_or > 0.0,
        "多数派の発言オッズが少数派を上回るべき (log-OR={log_or})"
    );
}

// --------------------------------------------------------------------------- //
// H5: 未来評価ギャップが正 (多数派が未来も楽観・少数派が悲観)
// --------------------------------------------------------------------------- //

#[test]
fn future_assessment_gap_is_positive() {
    let world = final_world(&base_config());
    let gap = future_assessment_gap(&world);
    assert!(gap > 0.0, "未来評価ギャップが正であるべき (gap={gap})");
}

// --------------------------------------------------------------------------- //
// ハードコア境界: hardcore_frac を上げると少数派ハードコアの生存率が高い
// --------------------------------------------------------------------------- //

#[test]
fn high_hardcore_frac_sustains_minority_hardcore() {
    let cfg = Config {
        hardcore_frac: 0.25,
        ..base_config()
    };
    let world = final_world(&cfg);
    let surv = hardcore_survival(&world);
    assert!(
        surv > 0.5,
        "hardcore_frac=0.25 では少数派ハードコアが大半生存すべき (surv={surv})"
    );
}

// --------------------------------------------------------------------------- //
// 網モデル切替
// --------------------------------------------------------------------------- //

#[test]
fn all_network_models_run() {
    for model in [
        NetworkModel::WattsStrogatz,
        NetworkModel::ErdosRenyi,
        NetworkModel::BarabasiAlbert,
    ] {
        let cfg = Config {
            n: 300,
            network_model: model,
            t_max: 30,
            seed: Some(7),
            ..Config::default()
        };
        let r = run(&cfg);
        assert_eq!(
            r.snapshots[0].len(),
            300,
            "{:?}: 初期状態が記録されるべき",
            model
        );
        assert!(r.final_tick > 0, "{:?}: 少なくとも 1 tick 進むべき", model);
    }
}

// --------------------------------------------------------------------------- //
// メディア均質性 η_m=0 (ノイズのみ) でも実行が完了する
// --------------------------------------------------------------------------- //

#[test]
fn zero_media_homogeneity_runs() {
    let cfg = Config {
        eta_m: 0.0,
        ..base_config()
    };
    let r = run(&cfg);
    assert!(r.final_tick > 0);
}
