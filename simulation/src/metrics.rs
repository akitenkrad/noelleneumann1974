//! 「沈黙の螺旋」固有の評価指標 (ローカル実装) と tick ごとの集計構造体．
//!
//! 正準統計 (mean / variance / shannon_entropy / hhi / distinct_clusters) は
//! `socsim-metrics::stats` を流用し，論文固有の意味を持つ指標
//! (`majority_voice_ratio` 等) はここで計算する．

use serde::Serialize;
use socsim_metrics::network::cascade_size;
use socsim_metrics::stats;

use crate::world::{Expression, SpiralWorld};

/// 1 tick の集計メトリクス．
///
/// フィールドは 1 つずつ runvault の指標になる (`record::log_step`)．旧 wide な
/// `metrics.csv` の列名をそのまま指標名にしてあるので，移行の前後で数を突き合わせ
/// られる．
#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    /// tick．
    pub t: usize,
    /// 発言量 `voice_volume = #{e≠Silence}/n`（Table 1 の 36%）．
    pub voice_volume: f64,
    /// 陣営別発言意欲の対数オッズ比（H1 中核アンカー: 53% vs 28%）．
    pub majority_voice_ratio: f64,
    /// 見かけ支持率 − 真の支持率 `q̂ − q`（H3 の創発）．
    pub perceived_minus_actual: f64,
    /// 未来評価ギャップ `mean(π^fut | b>0) − mean(π^fut | b<0)`（H5: Table 5）．
    pub future_assessment_gap: f64,
    /// ハードコア継続発言比（少数派内で発言を続ける比）．
    pub hardcore_survival: f64,
    /// 公的言説の見かけ支持率 `q̂`．
    pub apparent_support: f64,
    /// 公的表出の占有クラス数（単峰化の指標）．
    pub opinion_clusters: usize,
    /// 公的表出符号のシャノンエントロピー (発言の多様性)．
    pub expression_entropy: f64,
}

/// 発言量 `voice_volume`．
pub fn voice_volume(world: &SpiralWorld) -> f64 {
    let n = world.n();
    if n == 0 {
        return 0.0;
    }
    let voicing = world.e_pub.iter().filter(|e| e.is_voicing()).count();
    voicing as f64 / n as f64
}

/// 多数派 / 少数派の発言意欲の対数オッズ比 `majority_voice_ratio`（H1 中核アンカー）．
///
/// `log[ (多数派の自陣営発言オッズ) / (少数派の自陣営発言オッズ) ]`．多数派 = 真値で
/// 優勢な側 (`true_support>=0.5` なら賛成派 `b>0`，それ未満なら反対派 `b<0`)．論文の
/// 「勝ち組 53% vs 負け組 28%」(Table 2) の発言意欲非対称をどちらの符号が多数派でも
/// 同方向 (正) で測れるよう，符号ではなく多数派/少数派の役割で定義する．log-OR > 0 は
/// 「多数派の方が発言意欲が高い」(沈黙の螺旋) を意味する．0 除算・log(0) を避けるため
/// Laplace 平滑化 (+0.5/+1) する．
pub fn majority_voice_ratio(world: &SpiralWorld) -> f64 {
    let (maj_voice, min_voice) = voice_by_camp(world);
    let n = world.n();
    // 役割別の人数 (平滑化の分母に使う)．
    let minority_is_con = world.true_support() >= 0.5;
    let mut maj_total = 0usize;
    let mut min_total = 0usize;
    for &b in &world.b_priv {
        if b == 0.0 {
            continue;
        }
        let is_con = b < 0.0;
        if is_con == minority_is_con {
            min_total += 1;
        } else {
            maj_total += 1;
        }
    }
    let _ = n;
    // 比率 → Laplace 平滑化オッズ．
    let smoothed_odds = |rate: f64, total: usize| -> f64 {
        let voice = (rate * total as f64).round();
        let p = (voice + 0.5) / (total as f64 + 1.0);
        p / (1.0 - p)
    };
    let maj_odds = smoothed_odds(maj_voice, maj_total);
    let min_odds = smoothed_odds(min_voice, min_total);
    (maj_odds / min_odds).ln()
}

/// 見かけ支持率 − 真の支持率 `perceived_minus_actual = q̂ − q`．
pub fn perceived_minus_actual(world: &SpiralWorld) -> f64 {
    let q = world.true_support();
    let q_hat = world.apparent_support().unwrap_or(q);
    q_hat - q
}

/// 未来評価ギャップ `future_assessment_gap`（H5: Table 5 の 70% vs 23%）．
///
/// 多数派の平均未来知覚多数度 − 少数派の平均未来知覚多数度．各エージェントの
/// `π^fut_i` は「自意見側の未来多数度」なので，両派とも自分側が増えると見るほど
/// 大きい．多数派が未来も楽観し，少数派が悲観するほどギャップは大きくなる（H5）．
/// どちらの符号が多数派でも同方向 (正) で測れるよう，符号ではなく役割で定義する．
pub fn future_assessment_gap(world: &SpiralWorld) -> f64 {
    let minority_is_con = world.true_support() >= 0.5;
    let mut maj_sum = 0.0;
    let mut maj_n = 0usize;
    let mut min_sum = 0.0;
    let mut min_n = 0usize;
    for (i, &b) in world.b_priv.iter().enumerate() {
        if b == 0.0 {
            continue;
        }
        let is_con = b < 0.0;
        if is_con == minority_is_con {
            min_sum += world.pi_fut[i];
            min_n += 1;
        } else {
            maj_sum += world.pi_fut[i];
            maj_n += 1;
        }
    }
    let maj_mean = if maj_n > 0 {
        maj_sum / maj_n as f64
    } else {
        0.0
    };
    let min_mean = if min_n > 0 {
        min_sum / min_n as f64
    } else {
        0.0
    };
    maj_mean - min_mean
}

/// ハードコア継続発言比 `hardcore_survival`．
///
/// **少数派のハードコア** (劣勢側 ∩ 低閾値 θ_i < `HARDCORE_THETA_MAX`) のうち，現在
/// も自陣営として発言しているエージェントの比率．螺旋に抗して沈黙を拒む動員度の高い
/// 少数派 = ハードコアの生存率 (論文「17--25% の少数派が多数派より発言意欲高」)．
/// ハードコアが一人もいなければ少数派全体の発言比にフォールバックする．
pub fn hardcore_survival(world: &SpiralWorld) -> f64 {
    /// ハードコアと見なす閾値上限 (init で下裾を ×0.1 圧縮するため十分小さい)．
    const HARDCORE_THETA_MAX: f64 = 0.15;
    let minority_is_con = world.true_support() >= 0.5;

    let mut hc_total = 0usize;
    let mut hc_surviving = 0usize;
    let mut min_total = 0usize;
    let mut min_surviving = 0usize;
    for (i, &b) in world.b_priv.iter().enumerate() {
        let in_minority = if minority_is_con { b < 0.0 } else { b > 0.0 };
        if !in_minority {
            continue;
        }
        let voicing_own = if minority_is_con {
            world.e_pub[i] == Expression::VoiceCon
        } else {
            world.e_pub[i] == Expression::VoicePro
        };
        min_total += 1;
        if voicing_own {
            min_surviving += 1;
        }
        if world.voice_threshold[i] < HARDCORE_THETA_MAX {
            hc_total += 1;
            if voicing_own {
                hc_surviving += 1;
            }
        }
    }
    if hc_total > 0 {
        hc_surviving as f64 / hc_total as f64
    } else if min_total > 0 {
        min_surviving as f64 / min_total as f64
    } else {
        0.0
    }
}

/// 公的表出の占有クラス数 (符号コード {-1,0,1} 上の `distinct_clusters`)．
pub fn opinion_clusters(world: &SpiralWorld) -> usize {
    let codes: Vec<f64> = world.e_pub.iter().map(|e| e.code() as f64).collect();
    stats::distinct_clusters(&codes, 0.5)
}

/// 公的表出符号のシャノンエントロピー（3 カテゴリの分布から）．
pub fn expression_entropy(world: &SpiralWorld) -> f64 {
    let mut counts = [0.0f64; 3]; // pro / silence / con
    for e in &world.e_pub {
        match e {
            Expression::VoicePro => counts[0] += 1.0,
            Expression::Silence => counts[1] += 1.0,
            Expression::VoiceCon => counts[2] += 1.0,
        }
    }
    stats::shannon_entropy(&counts)
}

/// カスケード検出: 網上で「発言中」ノードの連結到達数（`network::cascade_size`）が
/// 全体の `frac` 超なら true．大域的螺旋 (発言の連結カスケード) の指標．
pub fn cascade_detected(world: &SpiralWorld, frac: f64) -> bool {
    let size = cascade_size(&world.network, |id| world.e_pub[id.0 as usize].is_voicing());
    let n = world.n().max(1);
    (size as f64 / n as f64) > frac
}

/// GDR 承認シナリオ用: 各派の平均 `π^now` (Table 4)．
/// 返り値は `(mean π^now | b>0, mean π^now | b<0)`．
pub fn pi_now_by_camp(world: &SpiralWorld) -> (f64, f64) {
    let mut pro_sum = 0.0;
    let mut pro_n = 0usize;
    let mut con_sum = 0.0;
    let mut con_n = 0usize;
    for (i, &b) in world.b_priv.iter().enumerate() {
        if b > 0.0 {
            pro_sum += world.pi_now[i];
            pro_n += 1;
        } else if b < 0.0 {
            con_sum += world.pi_now[i];
            con_n += 1;
        }
    }
    let pro = if pro_n > 0 {
        pro_sum / pro_n as f64
    } else {
        0.0
    };
    let con = if con_n > 0 {
        con_sum / con_n as f64
    } else {
        0.0
    };
    (pro, con)
}

/// 多数派 / 少数派の発言意欲（陣営別 voice_volume）を返す．
/// `(majority_voice, minority_voice)`（Table 2: 53% / 28%）．
pub fn voice_by_camp(world: &SpiralWorld) -> (f64, f64) {
    let minority_is_con = world.true_support() >= 0.5;
    let mut maj_total = 0usize;
    let mut maj_voice = 0usize;
    let mut min_total = 0usize;
    let mut min_voice = 0usize;
    for (i, &b) in world.b_priv.iter().enumerate() {
        if b == 0.0 {
            continue;
        }
        let is_con = b < 0.0;
        let in_minority = is_con == minority_is_con;
        let voicing_own = if is_con {
            world.e_pub[i] == Expression::VoiceCon
        } else {
            world.e_pub[i] == Expression::VoicePro
        };
        if in_minority {
            min_total += 1;
            if voicing_own {
                min_voice += 1;
            }
        } else {
            maj_total += 1;
            if voicing_own {
                maj_voice += 1;
            }
        }
    }
    let maj = if maj_total > 0 {
        maj_voice as f64 / maj_total as f64
    } else {
        0.0
    };
    let min = if min_total > 0 {
        min_voice as f64 / min_total as f64
    } else {
        0.0
    };
    (maj, min)
}

impl Metrics {
    /// 世界状態から 1 tick 分のメトリクスを計算する．
    pub fn compute(world: &SpiralWorld, t: usize) -> Self {
        Metrics {
            t,
            voice_volume: voice_volume(world),
            majority_voice_ratio: majority_voice_ratio(world),
            perceived_minus_actual: perceived_minus_actual(world),
            future_assessment_gap: future_assessment_gap(world),
            hardcore_survival: hardcore_survival(world),
            apparent_support: world.apparent_support().unwrap_or(0.0),
            opinion_clusters: opinion_clusters(world),
            expression_entropy: expression_entropy(world),
        }
    }
}
