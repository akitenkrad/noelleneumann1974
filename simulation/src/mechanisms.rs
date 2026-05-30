//! 「沈黙の螺旋」のメカニズム群 (socsim 6-phase ループ)．
//!
//! 論文の各規則と本研究固有の拡張を，socsim の固定 6-phase
//! (`PreStep → Environment → Decision → Interaction → Reward → PostStep`) へ配置
//! する．`prefalse_cascade` は `socsim-mechanisms` の
//! [`PerAgentThresholdContagionMechanism`](socsim_mechanisms::PerAgentThresholdContagionMechanism)
//! を直接流用する (世界状態が `BinaryState + Neighbors + ActivationThreshold` を実装済み，
//! per-agent 閾値 `θ_i` を世界状態から読む) ため，本モジュールには含めない
//! (`simulation.rs` で配線する)．
//!
//! | Mechanism | Phase |
//! |-----------|-------|
//! | [`MediaSignalMechanism`] | Environment |
//! | [`IssueSalienceMechanism`] | Environment |
//! | [`FearAppraisalMechanism`] | Decision |
//! | [`FutureAssessmentMechanism`] | Decision |
//! | [`VoiceDecisionMechanism`] | Decision |
//! | [`SilenceSpiralMechanism`] | Interaction |
//! | (`PerAgentThresholdContagionMechanism`) | Interaction |
//! | (`MetricsMechanism`) | Reward |
//! | [`ClimateQuasiStatMechanism`] | PostStep |

use rand::Rng;

use socsim_core::{AgentId, Mechanism, Phase, Result, StepContext};

use crate::world::{Expression, SpiralWorld};

/// ロジスティック関数 `logit^{-1}(x) = 1/(1+e^{-x})`．
#[inline]
pub fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// `[0,1]` クリップ．
#[inline]
fn clip01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// 発言オラクル: ルール / LLM の共通インタフェース
// ---------------------------------------------------------------------------

/// あるエージェントの発言確率を返すオラクル．
///
/// `VoiceDecisionMechanism` はこのトレイト越しに発言確率を得る．ルールベース版は
/// [`RuleOracle`] が §4.3 のロジットで，LLM 版 (`crate::llm`) は内省プロンプト →
/// 応答パースで返す．これにより `voice_decision` 本体 (スナップショット→一括書戻し
/// の同期更新) を共有できる．
pub trait VoiceOracle {
    /// エージェント `i` が「自陣営として発言する」確率 `s_i ∈ [0,1]` を返す．
    fn voice_prob(&mut self, world: &SpiralWorld, i: usize, cascade_pressure: bool) -> f64;
}

/// ルールベースのロジット発言オラクル (§4.3 の式)．
pub struct RuleOracle {
    pub beta_0: f64,
    pub beta_b: f64,
    pub beta_u: f64,
    pub beta_fear: f64,
    pub beta_pi: f64,
    pub beta_theta: f64,
    pub alpha: f64,
    pub alpha_a: f64,
}

impl VoiceOracle for RuleOracle {
    fn voice_prob(&mut self, world: &SpiralWorld, i: usize, cascade_pressure: bool) -> f64 {
        let b = world.b_priv[i];
        let f = world.fear[i];
        let u = world.media_signal;
        let pi_mix = (1.0 - self.alpha) * world.pi_now[i] + self.alpha * world.pi_fut[i];
        // 私的意見の符号方向に媒体・気候の寄与を合わせる (賛成派は u>0・π大で発言↑，
        // 反対派は u<0・π大で発言↑)．b の符号で媒体項の向きを揃える．
        let media_term = self.beta_u * u * b.signum();
        let cascade_term = if cascade_pressure {
            self.beta_theta
        } else {
            0.0
        };
        // ハードコアボーナス: 閾値 θ_i が低い (動員度の高い) 個体は気候に抗して発言
        // し続ける ([kuran:1995] のハードコア)．低 θ に集中した強い加点で，少数派でも
        // 沈黙を拒むハードコア境界を表現する．非ハードコア (θ≳0.3) にはほぼ効かない
        // よう急峻な関数 `4(1-θ)^6` を用いる (θ=0 で +4β_θ，θ=0.3 で +0.47β_θ)．
        let r = 1.0 - world.voice_threshold[i];
        let hardcore_bonus = self.beta_theta * 4.0 * r.powi(6);
        let logit = self.beta_0 + self.beta_b * b.abs() + media_term
            - self.beta_fear * f * (1.0 - self.alpha_a)
            + self.beta_pi * (pi_mix - 0.5) * 2.0
            + cascade_term
            + hardcore_bonus;
        sigmoid(logit)
    }
}

// ---------------------------------------------------------------------------
// Environment: media_signal
// ---------------------------------------------------------------------------

/// 媒体シグナル更新 (`Environment`，本研究固有)．
///
/// 均質性 `η_m` に従って外生意見シグナル `u_m(t) ∈ [-1,1]` を更新する．`η_m=1` で
/// 完全均質 (一定方向の強いシグナル＝マスメディアの世論創出)，`η_m=0` で寄与なし
/// (ノイズのみ平均 0)．`ctx.rng` (= RNG_MEDIA ストリーム) で揺らぎを引く．
///
/// `target_sign`: メディアが推す側 (+1 = 賛成側を推す)．多数派を増幅する想定で
/// 既定は真の多数派側を呼び出し側が設定する．
pub struct MediaSignalMechanism {
    pub eta_m: f64,
    pub target_sign: f64,
}

impl Mechanism<SpiralWorld> for MediaSignalMechanism {
    fn name(&self) -> &str {
        "media_signal"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Environment]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let eta = self.eta_m.clamp(0.0, 1.0);
        // 均質成分 (一定の世論創出シグナル) + (1-η) のノイズ．
        let noise: f64 = ctx.rng.gen_range(-1.0..1.0);
        let signal = eta * self.target_sign + (1.0 - eta) * noise;
        ctx.world.media_signal = signal.clamp(-1.0, 1.0);
        ctx.world.media_homogeneity = eta;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Environment: issue_salience_update
// ---------------------------------------------------------------------------

/// 争点顕在性更新 (`Environment`)．
///
/// 争点顕在性 `σ(t)` を緩やかに時間更新する (本モデルでは恐怖の基準点を僅かに動かす
/// だけの軽量実装)．`scratch` に "salience" を出す．大きな効果は持たせず，論文の
/// 「争点が顕在化するほど発言圧力が高まる」を将来拡張点として残す枠を提供する．
pub struct IssueSalienceMechanism {
    pub salience: f64,
}

impl Mechanism<SpiralWorld> for IssueSalienceMechanism {
    fn name(&self) -> &str {
        "issue_salience_update"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Environment]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        ctx.scratch.insert("salience", self.salience);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Decision: fear_appraisal
// ---------------------------------------------------------------------------

/// 孤立恐怖の評価更新 (`Decision`)．
///
/// 各 `i` が近傍の公的表出を見て「自分が少数側にいる」と知覚するほど恐怖 `f_i` が
/// 上方更新される (局所同調圧)．ハードコア (下裾 `θ_i`) は恐怖の上昇が緩い．
/// 恐怖は知覚気候 `π^now_i` (自意見側多数度) が低いほど高まる:
/// `f_i ← (1-μ) f_i + μ (1 - π^now_i) (1 - hardcore_relief_i)`．
pub struct FearAppraisalMechanism {
    /// 恐怖更新の学習率 μ．
    pub mu: f64,
}

impl Mechanism<SpiralWorld> for FearAppraisalMechanism {
    fn name(&self) -> &str {
        "fear_appraisal"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Decision]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let mu = self.mu.clamp(0.0, 1.0);
        let n = ctx.world.n();
        for i in 0..n {
            // 自意見側が劣勢 (π^now 低) ほど恐怖が高まる．閾値 θ_i が低いハードコアは
            // 同調圧に動じにくく恐怖の到達目標が低い (θ_i を係数に掛ける)．
            let theta = ctx.world.voice_threshold[i];
            let target = (1.0 - ctx.world.pi_now[i]) * theta;
            ctx.world.fear[i] = clip01((1.0 - mu) * ctx.world.fear[i] + mu * target);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Decision: future_assessment_update
// ---------------------------------------------------------------------------

/// 未来気候の外挿更新 (`Decision`，本研究固有・H4/H5 動的版)．
///
/// 直近 `W` tick の `voice_history` (自意見側の局所支持率) のトレンドを外挿し
/// `π^fut_i ← clip[0,1]( π^now_i + γ · Δπ̄_i )` で未来多数度を更新する．
/// 増勢にある側は未来を楽観し，衰勢にある側は悲観する (H5)．
pub struct FutureAssessmentMechanism {
    /// 外挿係数 γ．
    pub gamma: f64,
}

impl Mechanism<SpiralWorld> for FutureAssessmentMechanism {
    fn name(&self) -> &str {
        "future_assessment_update"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Decision]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let n = ctx.world.n();
        for i in 0..n {
            let hist = &ctx.world.voice_history[i];
            // 窓内の平均階差 Δπ̄．データ不足時は 0．
            let trend = if hist.len() >= 2 {
                let mut diffs = 0.0;
                for w in 1..hist.len() {
                    diffs += hist[w] - hist[w - 1];
                }
                diffs / (hist.len() - 1) as f64
            } else {
                0.0
            };
            ctx.world.pi_fut[i] = clip01(ctx.world.pi_now[i] + self.gamma * trend);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Decision: voice_decision
// ---------------------------------------------------------------------------

/// 発言決定 (`Decision`，理論の中核)．
///
/// 各エージェントが [`VoiceOracle`] から発言確率 `s_i` を得て，`ctx.rng` の
/// ベルヌーイ抽選で発言 / 沈黙を決める．**同期更新**: 旧 `π` のスナップショット
/// から全 `e_i` を計算し一括書き戻す (`hegselmann2005` と同じ適格集合スナップショット
/// 型)．発言する場合は私的意見 `b_i` の符号で賛成 / 反対を決める (心の中の意見を
/// 偽らず表明する; 沈黙は表明回避)．
pub struct VoiceDecisionMechanism<O: VoiceOracle> {
    pub oracle: O,
}

impl<O: VoiceOracle> Mechanism<SpiralWorld> for VoiceDecisionMechanism<O> {
    fn name(&self) -> &str {
        "voice_decision"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Decision]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let n = ctx.world.n();
        // 近傍発言比 ρ^V_i > θ_i のカスケード圧 (旧 e のスナップショットで判定)．
        let cascade_pressure: Vec<bool> = (0..n)
            .map(|i| {
                let id = AgentId(i as u64);
                let nbrs = ctx.world.neighbors_of_ids(id);
                if nbrs.is_empty() {
                    return false;
                }
                let voicing = nbrs
                    .iter()
                    .filter(|&&j| ctx.world.e_pub[j.0 as usize].is_voicing())
                    .count();
                let rho = voicing as f64 / nbrs.len() as f64;
                rho > ctx.world.voice_threshold[i]
            })
            .collect();

        // 発言確率を旧状態から一括計算．
        let probs: Vec<f64> = (0..n)
            .map(|i| self.oracle.voice_prob(ctx.world, i, cascade_pressure[i]))
            .collect();

        // ベルヌーイ抽選で新 e を一括書き戻し (同期更新)．
        let mut new_e = vec![Expression::Silence; n];
        for (i, e) in new_e.iter_mut().enumerate() {
            let voices = ctx.rng.gen::<f64>() < probs[i];
            *e = if !voices {
                Expression::Silence
            } else if ctx.world.b_priv[i] >= 0.0 {
                Expression::VoicePro
            } else {
                Expression::VoiceCon
            };
        }
        ctx.world.e_pub = new_e;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Interaction: silence_spiral (準統計的器官)
// ---------------------------------------------------------------------------

/// 準統計的器官による現在気候の更新 (`Interaction`)．
///
/// 各 `i` が近傍 `N(i)` の**公的表出**を観察し，自意見側の現在多数度 `π^now_i` を
/// 局所更新する:
/// `π^now_i ← (1-λ) π^now_i + λ · #{自陣営発言} / #{発言者}`（発言者がいなければ
/// 観測なしとして更新しない）．**準統計的器官は「誰が発言しているか」の構成比から
/// 気候を推し量る**ので分母は発言者のみとし，沈黙は気候の手がかりにしない (これが
/// ないと voice_volume が両陣営とも 0 へ自己崩壊する)．`hegselmann2005` の HK 機構を
/// 連続意見観察から離散表出観察へ組み替えた版である (私的 `b_j` は観測不可)．同期更新:
/// 旧 `e_j` を観察し新 `π_i` を一括計算．
pub struct SilenceSpiralMechanism {
    /// 気候更新の学習率 λ．
    pub lambda: f64,
}

impl Mechanism<SpiralWorld> for SilenceSpiralMechanism {
    fn name(&self) -> &str {
        "silence_spiral"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::Interaction]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let lambda = self.lambda.clamp(0.0, 1.0);
        let mut new_pi = ctx.world.pi_now.clone();
        for (i, pi) in new_pi.iter_mut().enumerate() {
            let id = AgentId(i as u64);
            let nbrs = ctx.world.neighbors_of_ids(id);
            if nbrs.is_empty() {
                continue;
            }
            let my_sign = ctx.world.b_priv[i].signum();
            // 発言者のうち自陣営として発言している比 (分母 = 発言者数)．
            let mut voicing = 0usize;
            let mut same = 0usize;
            for &j in &nbrs {
                match ctx.world.e_pub[j.0 as usize] {
                    Expression::VoicePro => {
                        voicing += 1;
                        if my_sign >= 0.0 {
                            same += 1;
                        }
                    }
                    Expression::VoiceCon => {
                        voicing += 1;
                        if my_sign < 0.0 {
                            same += 1;
                        }
                    }
                    Expression::Silence => {}
                }
            }
            if voicing == 0 {
                continue; // 発言者なし → 気候の手がかりなし (更新しない)．
            }
            let observed = same as f64 / voicing as f64;
            *pi = clip01((1.0 - lambda) * *pi + lambda * observed);
        }
        ctx.world.pi_now = new_pi;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PostStep: climate_quasi_stat
// ---------------------------------------------------------------------------

/// 気候の集約と収束判定 (`PostStep`)．
///
/// 全 tick の集約値 (見かけ支持率 `q̂`，知覚-真値乖離) を `voice_history` へ積み，
/// 各エージェントの自意見側局所支持率を `future_assessment` 用に記録する．公的支持率
/// が単峰化 (発言量がほぼ全員 or 一方に固まる) して安定したら `request_stop`．
pub struct ClimateQuasiStatMechanism {
    /// 未来外挿の窓幅 W．
    pub window: usize,
    /// 収束判定: 連続 `q̂` の変化がこの値未満なら停止候補．
    pub tol: f64,
    /// 直近の見かけ支持率 (収束判定用)．
    prev_q_hat: Option<f64>,
    /// 連続安定カウント．
    stable_count: usize,
}

impl ClimateQuasiStatMechanism {
    pub fn new(window: usize, tol: f64) -> Self {
        ClimateQuasiStatMechanism {
            window: window.max(1),
            tol,
            prev_q_hat: None,
            stable_count: 0,
        }
    }
}

impl Mechanism<SpiralWorld> for ClimateQuasiStatMechanism {
    fn name(&self) -> &str {
        "climate_quasi_stat"
    }
    fn phases(&self) -> &'static [Phase] {
        &[Phase::PostStep]
    }
    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, SpiralWorld>) -> Result<()> {
        let n = ctx.world.n();
        // 各エージェントの自意見側局所支持率を voice_history に積む (未来外挿用)．
        for i in 0..n {
            let id = AgentId(i as u64);
            let nbrs = ctx.world.neighbors_of_ids(id);
            let my_sign = ctx.world.b_priv[i].signum();
            let mut voicing = 0usize;
            let mut same = 0usize;
            for &j in &nbrs {
                match ctx.world.e_pub[j.0 as usize] {
                    Expression::VoicePro => {
                        voicing += 1;
                        if my_sign >= 0.0 {
                            same += 1;
                        }
                    }
                    Expression::VoiceCon => {
                        voicing += 1;
                        if my_sign < 0.0 {
                            same += 1;
                        }
                    }
                    Expression::Silence => {}
                }
            }
            // 発言者がいなければ現在気候を踏襲 (手がかりなし)．
            let local = if voicing == 0 {
                ctx.world.pi_now[i]
            } else {
                same as f64 / voicing as f64
            };
            let hist = &mut ctx.world.voice_history[i];
            hist.push_back(local);
            while hist.len() > self.window {
                hist.pop_front();
            }
        }

        // 見かけ支持率の安定で収束判定．
        let q_hat = ctx.world.apparent_support().unwrap_or(0.5);
        ctx.scratch.insert("apparent_support", q_hat);
        if let Some(prev) = self.prev_q_hat {
            if (q_hat - prev).abs() < self.tol {
                self.stable_count += 1;
            } else {
                self.stable_count = 0;
            }
        }
        self.prev_q_hat = Some(q_hat);
        // 5 tick 連続で安定したら停止．
        if self.stable_count >= 5 {
            ctx.request_stop();
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 近傍取得の補助 (Neighbors トレイトを使わず world から直接引く)
// ---------------------------------------------------------------------------

impl SpiralWorld {
    /// 近傍 ID を返す (mechanism 内の借用衝突を避ける薄ラッパ)．
    pub(crate) fn neighbors_of_ids(&self, id: AgentId) -> Vec<AgentId> {
        self.network.neighbors(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_monotone() {
        assert!(sigmoid(-10.0) < sigmoid(0.0));
        assert!(sigmoid(0.0) < sigmoid(10.0));
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn rule_oracle_high_climate_raises_prob() {
        use crate::config::Config;
        use crate::simulation::init_world;
        let cfg = Config {
            n: 50,
            seed: Some(1),
            ..Config::default()
        };
        let mut world = init_world(&cfg, 1);
        // 全員の現在・未来気候を高くする → 発言確率が上がるはず．
        let mut oracle = RuleOracle {
            beta_0: 0.0,
            beta_b: 2.0,
            beta_u: 1.0,
            beta_fear: 2.5,
            beta_pi: 3.5,
            beta_theta: 1.5,
            alpha: 0.7,
            alpha_a: 0.0,
        };
        world.pi_now = vec![0.1; world.n()];
        world.pi_fut = vec![0.1; world.n()];
        let low = oracle.voice_prob(&world, 0, false);
        world.pi_now = vec![0.9; world.n()];
        world.pi_fut = vec![0.9; world.n()];
        let high = oracle.voice_prob(&world, 0, false);
        assert!(high > low, "高い知覚気候は発言確率を上げる");
    }
}
