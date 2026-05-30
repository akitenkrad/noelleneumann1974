//! socsim フレームワーク上の「沈黙の螺旋」世界状態．
//!
//! `SpiralWorld` は socsim の [`WorldState`] を実装する．エージェントは固定位相の
//! ノード (移動なし) であり，網は [`SocialNetwork`] (Watts--Strogatz 既定，ER/BA も
//! 生成可) で保持する．空間モデルではないため `socsim-grid` は不使用である．
//!
//! Noelle-Neumann 理論の中核要件は **私的意見 `b_i` と公的表出 `e_i` の二層分離**
//! である．`silence_spiral` mechanism は近傍の**公的表出 `e_j` のみ**を観測対象と
//! し，私的意見 `b_j` は観測不可とする — これが H3 の「知覚気候と実分布の乖離」を
//! 生む創発の源である ([kuran:1995] の preference falsification と同型)．
//!
//! `prefalse_cascade` (ハードコア / 逆螺旋) のために [`BinaryState`] +
//! [`Neighbors`] を実装する (`granovetter1973` で確立した
//! [`ThresholdContagionMechanism`](socsim_mechanisms::ThresholdContagionMechanism)
//! 流用パターン)．`is_active` = 「現在発言中」を意味する．

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use socsim_core::{AgentId, BinaryState, Neighbors, SimClock, WorldState};
use socsim_net::SocialNetwork;

/// 公的表出 `e_i`：論争的話題について公に表明するか / 沈黙するか．
///
/// 私的意見 `b_i` の符号 (賛成 / 反対) とは独立に決まり得る (それが理論の核心)．
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expression {
    /// 賛成側 (`b_i > 0` 派) として公に発言する．
    VoicePro,
    /// 反対側 (`b_i < 0` 派) として公に発言する．
    VoiceCon,
    /// 沈黙する (公的に意見を表明しない)．
    Silence,
}

impl Expression {
    /// CSV / 可視化向けの整数コード (`+1` 賛成発言 / `-1` 反対発言 / `0` 沈黙)．
    pub fn code(self) -> i8 {
        match self {
            Expression::VoicePro => 1,
            Expression::VoiceCon => -1,
            Expression::Silence => 0,
        }
    }

    /// 発言しているか (賛成・反対のいずれか)．
    pub fn is_voicing(self) -> bool {
        !matches!(self, Expression::Silence)
    }
}

/// 「沈黙の螺旋」の世界状態．
///
/// `Clone + Serialize + Deserialize + Debug` を導出する (スナップショット・出力・
/// アブレーション用)．`SocialNetwork` は socsim-net が `Clone/Debug/Serialize/
/// Deserialize` を備える (edcc6eb)．
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpiralWorld {
    /// シミュレーションクロック．
    pub clock: SimClock,
    /// エージェント ID (`0..n`，ソート済み)．
    pub agents: Vec<AgentId>,
    /// 網トポロジ (WS 既定．ER/BA も生成可)．
    pub network: SocialNetwork,

    // ── 私的意見と公的表出を分離（理論の中核） ──────────────────────────────
    /// 私的意見 `b_i ∈ [-1,1]`（外生衝撃なき限り固定．正＝賛成側）．
    pub b_priv: Vec<f64>,
    /// 公的表出 `e_i ∈ {VoicePro, VoiceCon, Silence}`．
    pub e_pub: Vec<Expression>,

    // ── 準統計的器官による気候知覚（agent ごと） ──────────────────────────
    /// 自意見側の現在多数度 `π^now_i ∈ [0,1]`．
    pub pi_now: Vec<f64>,
    /// 自意見側の未来多数度 `π^fut_i ∈ [0,1]`．
    pub pi_fut: Vec<f64>,
    /// 直近 `W` tick の局所支持率 (未来外挿用)．
    pub voice_history: Vec<VecDeque<f64>>,

    // ── 個人差・閾値 ────────────────────────────────────────────────────────
    /// 孤立恐怖傾性 `f_i ∈ [0,1]`．
    pub fear: Vec<f64>,
    /// 表明閾値 `θ_i ∈ [0,1]` ([kuran:1995] 流)．下裾がハードコア．
    pub voice_threshold: Vec<f64>,

    // ── 外生媒体シグナル（マスメディアによる意見気候の創出） ────────────────
    /// 媒体シグナル `u_m(t) ∈ [-1,1]`．
    pub media_signal: f64,
    /// 媒体均質性 `η_m ∈ [0,1]`．
    pub media_homogeneity: f64,
}

impl SpiralWorld {
    /// エージェント数 n．
    pub fn n(&self) -> usize {
        self.agents.len()
    }

    /// `b_priv` の符号で見た真の賛成支持率 `q = #{b_i > 0} / n`．
    pub fn true_support(&self) -> f64 {
        if self.b_priv.is_empty() {
            return 0.0;
        }
        let pro = self.b_priv.iter().filter(|&&b| b > 0.0).count();
        pro as f64 / self.b_priv.len() as f64
    }

    /// 公的言説の見かけ支持率 `q̂ = #{VoicePro} / #{voicing}`（沈黙者を除外）．
    /// 発言者が一人もいなければ `None`．
    pub fn apparent_support(&self) -> Option<f64> {
        let voicing = self.e_pub.iter().filter(|e| e.is_voicing()).count();
        if voicing == 0 {
            return None;
        }
        let pro = self
            .e_pub
            .iter()
            .filter(|&&e| e == Expression::VoicePro)
            .count();
        Some(pro as f64 / voicing as f64)
    }
}

impl WorldState for SpiralWorld {
    fn agent_ids(&self) -> Vec<AgentId> {
        // すでにソート済み．契約 (sorted) を明示する．
        self.agents.clone()
    }

    fn clock(&self) -> &SimClock {
        &self.clock
    }

    fn clock_mut(&mut self) -> &mut SimClock {
        &mut self.clock
    }
}

impl Neighbors for SpiralWorld {
    /// 固定網の隣接そのもの (socsim-net の無向隣接)．
    fn neighbors_of(&self, id: AgentId) -> Vec<AgentId> {
        self.network.neighbors(id)
    }
}

impl BinaryState for SpiralWorld {
    /// active = 「現在発言中」(賛成・反対いずれか)．
    ///
    /// `prefalse_cascade` の [`ThresholdContagionMechanism`] はこの真偽値で
    /// 近傍発言比 `ρ^V_i` を測り，沈黙者を発言へ反転させる．
    fn is_active(&self, id: AgentId) -> bool {
        self.e_pub[id.0 as usize].is_voicing()
    }

    /// 沈黙者を発言へ反転させる (反転先は私的意見 `b_i` の符号で決める)．
    ///
    /// `active=false` への書き戻しは沈黙化を意味するが，ThresholdContagion は
    /// 単調増加 (発言のみ) なので実際には呼ばれない．
    fn set_active(&mut self, id: AgentId, active: bool) {
        let i = id.0 as usize;
        if active {
            // 賛成側私的意見なら賛成発言，それ以外は反対発言．
            self.e_pub[i] = if self.b_priv[i] >= 0.0 {
                Expression::VoicePro
            } else {
                Expression::VoiceCon
            };
        } else {
            self.e_pub[i] = Expression::Silence;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use socsim_core::SimRng;

    fn tiny_world() -> SpiralWorld {
        let ids: Vec<AgentId> = (0..6u64).map(AgentId).collect();
        let mut rng = SimRng::from_seed(7);
        let network = SocialNetwork::watts_strogatz(&ids, 2, 0.0, &mut rng);
        SpiralWorld {
            clock: SimClock::new(10),
            agents: ids,
            network,
            b_priv: vec![0.5, 0.5, 0.5, -0.5, -0.5, -0.5],
            e_pub: vec![
                Expression::VoicePro,
                Expression::Silence,
                Expression::VoicePro,
                Expression::Silence,
                Expression::VoiceCon,
                Expression::Silence,
            ],
            pi_now: vec![0.5; 6],
            pi_fut: vec![0.5; 6],
            voice_history: vec![VecDeque::new(); 6],
            fear: vec![0.5; 6],
            voice_threshold: vec![0.5; 6],
            media_signal: 0.0,
            media_homogeneity: 0.0,
        }
    }

    #[test]
    fn true_support_counts_positive_priv() {
        let w = tiny_world();
        assert!((w.true_support() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn apparent_support_excludes_silence() {
        let w = tiny_world();
        // 発言者: 2 pro + 1 con = 3 → q̂ = 2/3．
        assert!((w.apparent_support().unwrap() - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn binary_state_active_iff_voicing() {
        let w = tiny_world();
        assert!(w.is_active(AgentId(0)));
        assert!(!w.is_active(AgentId(1)));
    }
}
