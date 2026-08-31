//! 実行設定 (`Config`) と，runvault へ渡す実験条件のシリアライズ．

use serde::Serialize;

/// 網生成モデル．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkModel {
    /// Watts--Strogatz スモールワールド (既定)．
    WattsStrogatz,
    /// Erdős--Rényi ランダムグラフ．
    ErdosRenyi,
    /// Barabási--Albert スケールフリー．
    BarabasiAlbert,
}

impl NetworkModel {
    pub fn label(self) -> &'static str {
        match self {
            NetworkModel::WattsStrogatz => "watts-strogatz",
            NetworkModel::ErdosRenyi => "erdos-renyi",
            NetworkModel::BarabasiAlbert => "barabasi-albert",
        }
    }
}

/// 網モデル名のパース．
pub fn parse_network_model(s: &str) -> Result<NetworkModel, String> {
    match s.trim().to_lowercase().as_str() {
        "watts-strogatz" | "ws" | "small-world" => Ok(NetworkModel::WattsStrogatz),
        "erdos-renyi" | "er" | "random" => Ok(NetworkModel::ErdosRenyi),
        "barabasi-albert" | "ba" | "scale-free" => Ok(NetworkModel::BarabasiAlbert),
        other => Err(format!(
            "未知の網モデル: '{other}' (watts-strogatz / erdos-renyi / barabasi-albert)"
        )),
    }
}

/// 発言決定モード．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionMode {
    /// ルールベース・ロジスティック版 (既定．LLM 非呼び出し)．
    Rule,
    /// LLM 内省版 (Phase 3．`--features llm` 必須)．
    Llm,
}

impl DecisionMode {
    pub fn label(self) -> &'static str {
        match self {
            DecisionMode::Rule => "rule",
            DecisionMode::Llm => "llm",
        }
    }
}

/// 発言決定モード名のパース．
pub fn parse_decision_mode(s: &str) -> Result<DecisionMode, String> {
    match s.trim().to_lowercase().as_str() {
        "rule" | "logit" | "logistic" => Ok(DecisionMode::Rule),
        "llm" | "introspect" => Ok(DecisionMode::Llm),
        other => Err(format!("未知の決定モード: '{other}' (rule / llm)")),
    }
}

/// 単一 run の設定．
#[derive(Debug, Clone)]
pub struct Config {
    /// エージェント数 n．
    pub n: usize,
    /// 真の賛成支持率 q = #{b_i>0}/n（初期私的意見分布の制御）．
    pub true_support: f64,
    /// 網生成モデル．
    pub network_model: NetworkModel,
    /// 平均次数 k (WS) / BA の m / ER は期待次数として p に換算．
    pub network_k: usize,
    /// WS 再配線率 β．
    pub network_beta: f64,

    /// 媒体均質性 η_m ∈ [0,1]．
    pub eta_m: f64,
    /// 未来重み α ∈ (0.5,1.0]（H5）．
    pub alpha: f64,
    /// 気候係数 β_π (ロジット)．
    pub beta_pi: f64,
    /// 恐怖係数 β_f (ロジット)．
    pub beta_fear: f64,
    /// 匿名係数 α_a ∈ [0,1]（高匿名で恐怖縮減）．
    pub alpha_a: f64,
    /// ハードコア比率（下裾の閾値割合）．
    pub hardcore_frac: f64,

    /// 最大 tick 数 T．
    pub t_max: usize,
    /// 発言決定モード (rule / llm)．
    pub decision_mode: DecisionMode,
    /// 乱数シード (省略時はランダム)．
    pub seed: Option<u64>,
    /// 生成物 (`opinions.csv`) の出力先．runvault の run ディレクトリの
    /// `artifacts/` を呼び出し側が入れる．実験条件ではないので
    /// [`RunParameters`] には載せない．
    pub output_dir: String,

    // ── LLM 層設定 (rule モードでは無視される．設計書 §7) ──────────────────
    /// LLM 生成温度 (既定 0.0 = 擬似決定論)．
    pub llm_temperature: f32,
    /// LLM 生成シード (バックエンドへ渡す．Ollama は honor，OpenAI は best-effort)．
    pub llm_seed: u64,
    /// プロンプト→応答キャッシュの永続パス (`None` = in-memory)．
    /// 温暖キャッシュ再生 (cold→warm 100% cache-hit) のため LLM モードで永続化する．
    pub cache_path: Option<String>,

    // ── 固定的なモデル定数（CLI 非露出．設計書 §4.3） ──────────────────────
    /// ロジット切片 β_0．
    pub beta_0: f64,
    /// 私的意見係数 β_b．
    pub beta_b: f64,
    /// 媒体係数 β_u．
    pub beta_u: f64,
    /// カスケード閾値ボーナス係数 β_θ．
    pub beta_theta: f64,
    /// 気候更新の学習率 λ (silence_spiral)．
    pub lambda: f64,
    /// 未来外挿係数 γ (future_assessment)．
    pub gamma: f64,
    /// 未来外挿の窓幅 W．
    pub window: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            n: 1000,
            true_support: 0.37,
            network_model: NetworkModel::WattsStrogatz,
            network_k: 6,
            network_beta: 0.1,
            eta_m: 0.5,
            alpha: 0.7,
            beta_pi: 1.5,
            beta_fear: 1.6,
            alpha_a: 0.0,
            hardcore_frac: 0.05,
            t_max: 80,
            decision_mode: DecisionMode::Rule,
            seed: Some(42),
            output_dir: "results".to_string(),
            llm_temperature: 0.0,
            llm_seed: 0,
            cache_path: None,
            beta_0: -1.9,
            beta_b: 1.5,
            beta_u: 0.6,
            beta_theta: 1.4,
            lambda: 0.25,
            gamma: 2.0,
            window: 5,
        }
    }
}

/// 実験条件 (runvault の `config.json` の `parameters` に入る)．
///
/// どのサブコマンドの実行かは `run.json` の `subcommand` が持つので `command` は
/// 持たない．出力先も持たない — run ディレクトリが出力先そのものである．
/// `seed` は実体化済みの値で，`--seed` を省いた実行でも «実際に使われたシード» が残る．
#[derive(Serialize)]
pub struct RunParameters {
    pub n: usize,
    pub true_support: f64,
    pub network_model: String,
    pub network_k: usize,
    pub network_beta: f64,
    pub eta_m: f64,
    pub alpha: f64,
    pub beta_pi: f64,
    pub beta_fear: f64,
    pub alpha_a: f64,
    pub hardcore_frac: f64,
    pub t_max: usize,
    pub decision_mode: String,
    pub seed: u64,
    // モデル定数も記録 (再現性のため)．
    pub beta_0: f64,
    pub beta_b: f64,
    pub beta_u: f64,
    pub beta_theta: f64,
    pub lambda: f64,
    pub gamma: f64,
    pub window: usize,
}

impl Config {
    /// 実験条件へ変換する．`seed` は呼び出し側が実体化した値を渡す．
    pub fn to_parameters(&self, seed: u64) -> RunParameters {
        RunParameters {
            n: self.n,
            true_support: self.true_support,
            network_model: self.network_model.label().to_string(),
            network_k: self.network_k,
            network_beta: self.network_beta,
            eta_m: self.eta_m,
            alpha: self.alpha,
            beta_pi: self.beta_pi,
            beta_fear: self.beta_fear,
            alpha_a: self.alpha_a,
            hardcore_frac: self.hardcore_frac,
            t_max: self.t_max,
            decision_mode: self.decision_mode.label().to_string(),
            seed,
            beta_0: self.beta_0,
            beta_b: self.beta_b,
            beta_u: self.beta_u,
            beta_theta: self.beta_theta,
            lambda: self.lambda,
            gamma: self.gamma,
            window: self.window,
        }
    }
}
