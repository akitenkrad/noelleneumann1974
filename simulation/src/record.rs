//! runvault への記録の共通部分．
//!
//! 論文メタデータ (research) は `run` / `sweep` / `reproduce` のどのサブコマンドでも
//! 同一なので，ここ 1 箇所で組み立てる．ステップごとの指標の long 形式への落とし方，
//! 試行 1 本の終端行，条件 1 点ぶんの集約，`reproduce` の帯照合もここに集める．

use runvault::{Llm, Replication, Run, Target, Work};
use serde::Serialize;

use crate::metrics::Metrics;
use crate::simulation::{LlmUsage, SimulationResult};

/// runvault 上の実験名．`runvault path --experiment` に渡す値でもある．
/// バイナリ名 (`noelleneumann`) と揃える．
pub const EXPERIMENT: &str = "noelleneumann";
/// リポジトリの安定 id．git remote の名前とは独立に固定する．
pub const REPO_ID: &str = "noelleneumann1974";
/// 分野．初期意見・網・媒体シグナルで乱数を引くので `simulation`
/// (= `master_seed` が必須)．LLM 内省版でも測っているのはモデルの安全性ではなく
/// 網上の意見動学なので `llm-safety` ではない．LLM 側の同一性は `llm` ブロック
/// ([`llm_block`]) が持つ．
pub const DOMAIN: &str = "simulation";

/// 時間軸の単位．
///
/// 本モデルの時間は全エージェントを 6 phase で 1 巡する離散 tick なので，runvault の
/// 語彙では `step`．
pub const T_UNIT: &str = "step";

/// 指標の粒度．集団指標はどれも母集団全体の集約なので `run`．
const SCOPE: &str = "run";

/// 帯照合イベントの種別．コア語彙に無いので `x.<repo_id>.<name>` を使う．
const ANCHOR_EVENT: &str = "x.noelleneumann1974.anchor";

/// この再現実験が対象としている論文．
///
/// 主張 (沈黙の螺旋そのもの) と，`reproduce` が照合するアレンスバッハ調査の表を
/// 対象として並べる．`reference.csv` に論文の報告値を書くには，その値の出所である
/// target がここに宣言されている必要がある．
pub fn replication() -> Replication {
    Work::doi("10.1111/j.1460-2466.1974.tb00367.x")
        .title("The Spiral of Silence: A Theory of Public Opinion")
        .year(1974)
        .source_version("published")
        .target(Target::claim(
            "spiral-of-silence",
            "Perceiving one's own camp as the shrinking minority suppresses public expression, which distorts the climate others perceive",
        ))
        .target(Target::table("tbl1", "Table 1").condition("train test, total population"))
        .target(Target::table("tbl2", "Table 2").condition("coming of socialism, winners vs losers"))
        .target(Target::table("tbl4", "Table 4").condition("GDR recognition, present climate"))
        .target(Target::table("tbl5", "Table 5").condition("GDR recognition, future climate"))
        .obsidian_note(
            "研究/98_論文レポート/80-再現実験/実装完了/noelleneumann1974/設計書.md",
        )
}

// ---------------------------------------------------------------------------
// LLM ブロック
// ---------------------------------------------------------------------------

/// 実際に応答したバックエンドを `llm` ブロックに落とす．
///
/// `model` / `endpoint` はクライアントが名乗った値をそのまま使う．`provider` は
/// runvault の語彙ではなく自由記述なので，endpoint から «どのゲートウェイが答えたか»
/// を決める (`mock://…` はオフラインの scripted クライアント，それ以外はホスト名で
/// Ollama / OpenAI を分ける)．推測しているのは分類だけで，値そのものは記録から採る．
///
/// socsim-llm はスナップショット id を持たないので，持っていない値を作らずに
/// 名乗られた名前を書く．
pub fn llm_block(model: &str, endpoint: &str, temperature: f32) -> Llm {
    let provider = if endpoint.starts_with("mock://") {
        "mock"
    } else if endpoint.contains("openai") {
        "openai"
    } else {
        "ollama"
    };
    Llm {
        provider: provider.to_string(),
        model_snapshot: model.to_string(),
        temperature: Some(temperature as f64),
        // 発言内省のプロンプトはエージェントの状態ごとに組み立てられ，固定の
        // system prompt を持たない．無いものを hash しない．
        system_prompt_hash: None,
    }
}

// ---------------------------------------------------------------------------
// ステップごとの指標
// ---------------------------------------------------------------------------

/// シミュレーション 1 本ぶんの記録 (`run` / `reproduce` 用)．
///
/// ステップごとの 8 指標 (`t` は時間軸なので値としては書かない) と，run 全体を
/// 1 つの値で表す `converged` / `final_tick` を書く．実行時間は `status.json` の
/// `duration_sec` が正本なので指標にはしない．
pub fn log_simulation(run: &mut Run, result: &SimulationResult) {
    for m in &result.metrics_history {
        log_step(run, m);
    }
    run.log_metrics(
        SCOPE,
        &[
            ("converged", if result.converged { 1.0 } else { 0.0 }),
            ("final_tick", result.final_tick as f64),
        ],
    )
    .expect("run スコープの指標の記録に失敗");
}

/// [`Metrics`] の数値フィールドを 1 tick ぶんまとめて書く．
///
/// 旧 wide な `metrics.csv` の列がそのまま 1 本ずつの指標になる．名前は旧列名の
/// ままにしてあるので，移行の前後で «数が変わったか» を列名で直接突き合わせられる．
/// `opinion_clusters` は個数 (公的表出の占有クラス数) なので数であり，カテゴリでは
/// ない — 表出そのもののラベルは `artifacts/opinions.csv` の `e` 列が持つ．
fn log_step(run: &mut Run, m: &Metrics) {
    run.log_metrics_at(
        m.t as u64,
        T_UNIT,
        SCOPE,
        &[
            ("voice_volume", m.voice_volume),
            ("majority_voice_ratio", m.majority_voice_ratio),
            ("perceived_minus_actual", m.perceived_minus_actual),
            ("future_assessment_gap", m.future_assessment_gap),
            ("hardcore_survival", m.hardcore_survival),
            ("apparent_support", m.apparent_support),
            ("opinion_clusters", m.opinion_clusters as f64),
            ("expression_entropy", m.expression_entropy),
        ],
    )
    .unwrap_or_else(|e| panic!("t={} の指標の記録に失敗: {e}", m.t));
}

/// LLM 呼び出しの内訳 (旧 `llm_meta.json` の calls / cache_hits / cache_hit_rate)．
///
/// model / endpoint / temperature は数ではないので指標にはならない．そちらは
/// `run.json` の `llm` ブロック ([`llm_block`]) が持つ．
pub fn log_llm_usage(run: &mut Run, usage: &LlmUsage) {
    run.log_metrics(
        SCOPE,
        &[
            ("llm_calls", usage.calls as f64),
            ("llm_cache_hits", usage.cache_hits as f64),
            ("llm_cache_hit_rate", usage.cache_hit_rate),
        ],
    )
    .expect("LLM 呼び出しの内訳の記録に失敗");
}

// ---------------------------------------------------------------------------
// 終端イベント
// ---------------------------------------------------------------------------

/// `events.jsonl` に書く観測行．
///
/// 予約キーだけを持つ．数はここには書かない — ステップごとの値は `metrics.csv`
/// が，試行の最終値は下の [`TerminalEvent`] が正本なので，同じ数を 2 箇所に置くと
/// 食い違う余地ができる．この行が持つのは «その単位をいつ見たか» だけである．
///
/// `runvault verify --deep` は terminal の `unit_id` が observation にも現れ，
/// terminal の `t` がその単位の観測の最大 `t` と一致することを要求するので，観測した
/// 時刻を明示的に残す．
#[derive(Serialize)]
struct ObservationEvent<'a> {
    unit_id: &'a str,
    t: u64,
    t_unit: &'static str,
}

/// 観測 1 点を書く．
fn log_observation(run: &mut Run, unit_id: &str, t: u64) {
    run.log_event(
        "observation",
        &ObservationEvent {
            unit_id,
            t,
            t_unit: T_UNIT,
        },
    )
    .unwrap_or_else(|e| panic!("{unit_id} の t={t} の observation の記録に失敗: {e}"));
}

/// `events.jsonl` に書く終端行．
///
/// 先頭 6 フィールドは runvault の予約語 (`terminal` はこれを全部要求する)．残りは
/// 自由欄で，旧 `sweep_summary.csv` の 1 行がここに対応する．旧 CSV が持っていたのは
/// 最終 tick の値ではなく**定常状態 (後半平均)** の値なので，名前に `steady_` を付けて
/// 時刻 `t` の値と取り違えないようにする．
#[derive(Serialize)]
struct TerminalEvent<'a> {
    unit_id: &'a str,
    t: u64,
    t_unit: &'static str,
    outcome: &'static str,
    censored: bool,
    budget: u64,
    seed: u64,
    steady_voice_volume: f64,
    steady_majority_voice_ratio: f64,
    steady_perceived_minus_actual: f64,
    steady_future_assessment_gap: f64,
    steady_hardcore_survival: f64,
    steady_apparent_support: f64,
}

/// シミュレーション 1 本を `terminal` イベントとして書く．
///
/// 打ち切り (`censored`) の行は `t == budget` でなければならない．ドライバは
/// `ClimateQuasiStatMechanism` が気候の変化を見て停止を要求するまで回し，止まらなければ
/// `SimClock` の `t_max` まで進むので，収束しなかった run は必ず上限に達している．
/// この不変条件は runvault が `log_event` の書き込み時に検査するので，ここでは
/// 二重に持たない．
///
/// `observed` はこの単位を観測した時刻の列で，終端の `t` を必ず含む．`run` /
/// `reproduce` は全 tick を観測して `metrics.csv` に残すので全 tick を，`sweep` は
/// 各試行の定常状態しか見ないのでその最終 tick 1 点だけを渡す．
pub fn log_terminal(
    run: &mut Run,
    unit_id: &str,
    seed: u64,
    t_max: usize,
    observed: impl IntoIterator<Item = u64>,
    result: &SimulationResult,
    steady: &Metrics,
) {
    for t in observed {
        log_observation(run, unit_id, t);
    }

    let event = TerminalEvent {
        unit_id,
        t: result.final_tick as u64,
        t_unit: T_UNIT,
        outcome: if result.converged {
            "converged"
        } else {
            "unconverged"
        },
        censored: !result.converged,
        budget: t_max as u64,
        seed,
        steady_voice_volume: steady.voice_volume,
        steady_majority_voice_ratio: steady.majority_voice_ratio,
        steady_perceived_minus_actual: steady.perceived_minus_actual,
        steady_future_assessment_gap: steady.future_assessment_gap,
        steady_hardcore_survival: steady.hardcore_survival,
        steady_apparent_support: steady.apparent_support,
    };
    run.log_event("terminal", &event)
        .unwrap_or_else(|e| panic!("{unit_id} の terminal イベントの記録に失敗: {e}"));
}

// ---------------------------------------------------------------------------
// 条件 1 点ぶんの集約 (sweep の子 run)
// ---------------------------------------------------------------------------

/// 1 条件を回した試行 1 本の最終像．集約の材料になる．
pub struct TrialOutcome {
    /// 収束したか．
    pub converged: bool,
    /// 収束 (または打ち切り) した tick．
    pub final_tick: usize,
    /// 定常状態 (後半平均) の指標．
    pub steady: Metrics,
}

/// 1 条件 (η_m, β, α, k, q の 1 点) を 1 つの値で表す指標．
///
/// 試行ごとの値は `events.jsonl` の担当なので，ここには集約しか書かない．試行ごとの
/// `voice_volume` を指標にすると (`run_uid`, `step`, `scope`, `name`) が重複するので，
/// 散らばりが要る図は `events.jsonl` から組み直す．
pub fn log_condition_summary(run: &mut Run, trials: &[TrialOutcome]) {
    let n = trials.len();
    assert!(n > 0, "試行が 1 本もありません");
    let n_f = n as f64;

    let n_converged = trials.iter().filter(|t| t.converged).count();
    let mean = |f: &dyn Fn(&TrialOutcome) -> f64| trials.iter().map(f).sum::<f64>() / n_f;

    run.log_metrics(
        SCOPE,
        &[
            ("n_units", n_f),
            ("n_converged", n_converged as f64),
            ("convergence_rate", n_converged as f64 / n_f),
            ("mean_voice_volume", mean(&|t| t.steady.voice_volume)),
            (
                "mean_majority_voice_ratio",
                mean(&|t| t.steady.majority_voice_ratio),
            ),
            (
                "mean_perceived_minus_actual",
                mean(&|t| t.steady.perceived_minus_actual),
            ),
            (
                "mean_future_assessment_gap",
                mean(&|t| t.steady.future_assessment_gap),
            ),
            (
                "mean_hardcore_survival",
                mean(&|t| t.steady.hardcore_survival),
            ),
            (
                "mean_apparent_support",
                mean(&|t| t.steady.apparent_support),
            ),
            ("mean_final_tick", mean(&|t| t.final_tick as f64)),
        ],
    )
    .expect("run スコープの指標の記録に失敗");
}

// ---------------------------------------------------------------------------
// reproduce の帯照合
// ---------------------------------------------------------------------------

/// 1 指標の観測-帯-判定の三つ組．
///
/// `target_lo` / `target_hi` は **この再現実装が置いた帯**であって論文の報告値では
/// ない (ABM は動的均衡まで回るので，調査のクロスセクションと絶対水準が揃わない —
/// 設計書 §7 の翻訳注記)．論文が報告した値の方は [`log_paper_references`] が出典付きで
/// `reference.csv` に書く．両者を同じ行に並べると，後から «どちらが論文の数か» が
/// 見分けられなくなる．
#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    /// 指標名 (run スコープ指標の名前と同一)．
    pub indicator: String,
    /// 観測値．
    pub observed: f64,
    /// 帯の下限．
    pub target_lo: f64,
    /// 帯の上限 (`None` = 上限なし)．`f64::INFINITY` は JSON に書けない．
    pub target_hi: Option<f64>,
    /// 帯に入ったか．
    pub pass: bool,
}

impl Anchor {
    /// 下限と上限を持つ帯．
    pub fn band(indicator: &str, observed: f64, lo: f64, hi: f64) -> Self {
        Anchor {
            indicator: indicator.to_string(),
            observed,
            target_lo: lo,
            target_hi: Some(hi),
            pass: observed >= lo && observed <= hi,
        }
    }

    /// 下限だけの帯 (「> lo」)．
    pub fn at_least(indicator: &str, observed: f64, lo: f64) -> Self {
        Anchor {
            indicator: indicator.to_string(),
            observed,
            target_lo: lo,
            target_hi: None,
            pass: observed >= lo,
        }
    }
}

/// 観測量そのものは run 全体を 1 つの値で表す数なので指標に書く．
///
/// 帯照合の指標名にはどれも接頭辞が付く (`steady_` = 定常状態の後半平均，`final_` =
/// 最終 tick の世界状態，`boundary_` = ハードコア境界シナリオ)．接頭辞なしの名前は
/// ステップごとの指標が使っているので，付けないと «t を持つ行» と «持たない行» に
/// 同じ名前が並び，どちらの意味なのかが読めなくなる．
pub fn log_anchor_observations(run: &mut Run, anchors: &[Anchor]) {
    let values: Vec<(&str, f64)> = anchors
        .iter()
        .map(|a| (a.indicator.as_str(), a.observed))
        .collect();
    run.log_metrics(SCOPE, &values)
        .expect("帯照合の観測量の記録に失敗");

    let passed = anchors.iter().filter(|a| a.pass).count();
    run.log_metrics(
        SCOPE,
        &[
            ("anchors_passed", passed as f64),
            ("anchors_total", anchors.len() as f64),
        ],
    )
    .expect("帯照合の集計の記録に失敗");
}

/// 帯そのものと PASS / off の判定は数ではないので `events.jsonl` へ書く．
pub fn log_anchors(run: &mut Run, anchors: &[Anchor]) {
    for a in anchors {
        run.log_event(ANCHOR_EVENT, a)
            .unwrap_or_else(|e| panic!("帯照合 {} の記録に失敗: {e}", a.indicator));
    }
}

/// 論文が報告した値を出典付きで `reference.csv` に書く．
///
/// ここに並ぶのは Table 1 / 2 / 4 の**アレンスバッハ調査の報告値**だけである．H1 の
/// log-OR > 0.8，H5 の gap > 0.4，ハードコア生存 > 0.7 は本再現が置いた定性的な
/// アンカーであって論文の数ではないので，出典を要求するこのファイルには書かない．
///
/// 反対派の `pi_now` だけは論文が «現在は賛成多数» を 57% の補数として報告している
/// ため，その算術を `source` に明示して残す (図から目測した値ではない)．
pub fn log_paper_references(run: &mut Run) {
    let refs: [(&str, f64, &str, &str); 5] = [
        (
            "steady_voice_volume",
            0.36,
            "tbl1",
            "Table 1: train test, willing to discuss 36% (N=9,966)",
        ),
        (
            "final_majority_voice",
            0.53,
            "tbl2",
            "Table 2: coming of socialism, winners 53% (N=229, 1972/8)",
        ),
        (
            "final_minority_voice",
            0.28,
            "tbl2",
            "Table 2: coming of socialism, losers 28%",
        ),
        (
            "final_pi_now_pro",
            0.49,
            "tbl4",
            "Table 4: GDR recognition 1971/1, supporters answering \"majority is now in favour\" 49%",
        ),
        (
            "final_pi_now_con",
            0.43,
            "tbl4",
            "Table 4: GDR recognition 1971/1, opponents answering \"majority is now in favour\" = 1 - 57%",
        ),
    ];
    for (name, value, target, source) in refs {
        run.log_reference(name, value)
            .target(target)
            .source(source)
            .send()
            .unwrap_or_else(|e| panic!("{name} の論文値の記録に失敗: {e}"));
    }
}

// ---------------------------------------------------------------------------
// シードの派生
// ---------------------------------------------------------------------------

/// 試行 1 本のシードを base seed から決定的に派生させる．
///
/// `master_seed` として記録するのは `base` の方で，実際に各試行が使うシードはこれで
/// 作る．引数の並びは移行前の `cmd_sweep` と同一で，`(base, 条件, index)` が同じなら
/// 常に同じ値を返す — この性質が壊れると，記録した `master_seed` から run を組み直せ
/// なくなるうえ，移行の前後で数が一致しなくなる．
#[allow(clippy::too_many_arguments)]
pub fn trial_seed(
    base: u64,
    eta_m: f64,
    network_beta: f64,
    alpha: f64,
    network_k: usize,
    true_support: f64,
    index: usize,
) -> u64 {
    socsim_core::derive_seed(
        base,
        &[
            eta_m.to_bits(),
            network_beta.to_bits(),
            alpha.to_bits(),
            network_k as u64,
            true_support.to_bits(),
            index as u64,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::trial_seed;

    fn seed_of(index: usize) -> u64 {
        trial_seed(42, 0.5, 0.1, 0.7, 6, 0.37, index)
    }

    #[test]
    fn same_inputs_give_the_same_seed() {
        for index in 0..8 {
            assert_eq!(
                seed_of(index),
                seed_of(index),
                "index={index} で再現しなかった"
            );
        }
    }

    #[test]
    fn each_coordinate_changes_the_seed() {
        let base = seed_of(0);
        assert_ne!(
            base,
            trial_seed(43, 0.5, 0.1, 0.7, 6, 0.37, 0),
            "base が効いていない"
        );
        assert_ne!(
            base,
            trial_seed(42, 0.75, 0.1, 0.7, 6, 0.37, 0),
            "eta_m が効いていない"
        );
        assert_ne!(
            base,
            trial_seed(42, 0.5, 0.3, 0.7, 6, 0.37, 0),
            "network_beta が効いていない"
        );
        assert_ne!(
            base,
            trial_seed(42, 0.5, 0.1, 0.8, 6, 0.37, 0),
            "alpha が効いていない"
        );
        assert_ne!(
            base,
            trial_seed(42, 0.5, 0.1, 0.7, 8, 0.37, 0),
            "network_k が効いていない"
        );
        assert_ne!(
            base,
            trial_seed(42, 0.5, 0.1, 0.7, 6, 0.50, 0),
            "true_support が効いていない"
        );
        assert_ne!(base, seed_of(1), "index が効いていない");
    }

    #[test]
    fn one_condition_gives_distinct_seeds_across_trials() {
        let seeds: std::collections::BTreeSet<u64> = (0..64).map(seed_of).collect();
        assert_eq!(seeds.len(), 64, "同一条件の試行でシードが衝突した");
    }

    /// 具体値を固定する．
    ///
    /// ここが変わるのは socsim の `derive_seed` が変わったときで，そのときは過去の
    /// run と結果を比較できなくなっている．`Cargo.lock` が socsim の commit を固定して
    /// いるので，この値は依存を上げたときにだけ動く．
    #[test]
    fn golden_values_are_pinned() {
        assert_eq!(seed_of(0), 7_040_709_732_673_663_151);
        assert_eq!(seed_of(1), 7_040_708_633_162_034_940);
        assert_eq!(
            trial_seed(42, 0.0, 0.0, 0.5, 6, 0.37, 0),
            6_655_718_133_916_044_139
        );
    }
}
