//! Noelle-Neumann (1974)「The Spiral of Silence」再現実装ライブラリ．
//!
//! 私的意見 `b_i` と公的表出 `e_i` の二層分離を貫いた意見動学を socsim の
//! WorldState + Mechanism (6-phase ループ) へ翻訳する．`run` (ルールベース) と
//! 任意の LLM 内省版 (feature `llm`) を提供する．統合テスト・例から使う公開 API を
//! ここで束ねる．

pub mod config;
pub mod mechanisms;
pub mod metrics;
pub mod prompts;
pub mod simulation;
pub mod world;

/// Phase-3 LLM 内省版 (feature `llm` 時のみ)．socsim-llm 共有ハーネスの薄いシム．
#[cfg(feature = "llm")]
pub mod llm;
