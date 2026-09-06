# アーキテクチャ

[English](architecture.md) | [日本語](architecture.ja.md)

## リポジトリ構成

```
noelleneumann1974/
├── simulation/                    # Rust crate `noelleneumann-simulation` (bin `noelleneumann`)
│   ├── src/
│   │   ├── main.rs                # clap CLI: run / sweep / reproduce
│   │   ├── config.rs              # Config / NetworkModel / DecisionMode / RunParameters
│   │   ├── world.rs               # SpiralWorld (WorldState + Neighbors + BinaryState) / Expression
│   │   ├── mechanisms.rs          # media_signal / issue_salience / fear_appraisal /
│   │   │                          #   future_assessment / voice_decision / silence_spiral /
│   │   │                          #   climate_quasi_stat + VoiceOracle トレイト + RuleOracle
│   │   ├── metrics.rs             # 論文固有指標 + tick ごとの Metrics 構造体
│   │   ├── simulation.rs          # init_world + run ドライバ (SimulationBuilder 配線)
│   │   ├── record.rs              # runvault: 論文メタデータ / long 形式の指標 /
│   │   │                          #   terminal・observation イベント / 帯照合 / 派生シード
│   │   ├── llm.rs                 # (feature `llm`) socsim-llm の薄い re-export シム
│   │   ├── prompts.rs             # LLM 内省プロンプト + 応答パーサ
│   │   └── lib.rs                 # テスト / 例向けの公開 API
│   ├── examples/mock_smoke.rs     # (feature `llm`) オフライン scripted-LLM スモーク
│   └── tests/                     # integration_test.rs + llm_mock_test.rs
├── tools/src/noelleneumann_tools/ # Python パッケージ `noelleneumann-tools`
├── docs/                          # 本ドキュメント (bilingual)
└── results/                       # runvault の results ルート (gitignore 対象)
    └── noelleneumann/             #   run 1 本 1 ディレクトリ．命名は runvault
```

## socsim フレームワーク

本体は [socsim](https://github.com/akitenkrad/rs-social-simulation-tools) (Rust ABM ツールキット) 上に構築する．網モデル + 意見動学なので以下に依存する．

- `socsim-core` — `WorldState` / `Mechanism` / `SimClock` / `SimRng` / `derive_seed` と能力トレイト (`Neighbors`, `BinaryState`, `ActivationThreshold`)．
- `socsim-engine` — `SimulationBuilder`，6 フェーズの step ループ，`RandomActivationScheduler`．
- `socsim-net` — `SocialNetwork` (Watts--Strogatz 既定．ER / BA も切替可)．
- `socsim-mechanisms` — `PerAgentThresholdContagionMechanism`．ハードコア / 選好偽装カスケードへ直接流用 (各エージェントの per-agent 閾値 θ_i を世界状態の `ActivationThreshold` から読む)．
- `socsim-metrics` (features `core`, `network`) — 正準 `stats` (mean / variance / shannon_entropy / hhi / distinct_clusters) と `network::cascade_size`．
- `socsim-llm` (features `live`) — 任意の `llm` Cargo feature でのみコンパイルされる内省 ablation 用．

出力の置き場は socsim の仕事ではなくなった: run ディレクトリの作成・命名と `config.json` / `metrics.csv` / `events.jsonl` / `reference.csv` / `status.json` / `manifest.csv` は [runvault](https://github.com/akitenkrad/rs-runvault) が持つ．本クレートはタイムスタンプ付きディレクトリ名も `latest` シンボリックリンクも自分では作らない．

## 二層の世界

`SpiralWorld` は理論の中核を保持する．私的意見ベクトル `b_priv` と，それと独立な公的表出ベクトル `e_pub` である．`silence_spiral` 機構は近傍の `e_pub` のみを観測し，私的意見は観測不可とする — これが知覚支持率と実支持率の乖離 (H3．選好偽装と同型) を生む．

世界は各エージェントの自意見側の現在 / 未来知覚多数度 (`pi_now` / `pi_fut`)，トレンド外挿用の局所支持率履歴 (`voice_history`)，孤立恐怖傾性 (`fear`) と表明閾値 (`voice_threshold`．下裾がハードコア)，スカラの媒体シグナルと均質性も保持する．

## 6 フェーズ上の 9 機構

| Mechanism | Phase | 役割 |
|-----------|-------|------|
| `media_signal` | Environment | 外生意見シグナル `u_m(t)` を更新．均質性 `η_m` が世論創出シグナルとノイズを混合する． |
| `issue_salience_update` | Environment | 争点顕在性 `σ(t)` を保持 (軽量．拡張点)． |
| `fear_appraisal` | Decision | 知覚気候から孤立恐怖 `f_i` を更新．ハードコア (低閾値) は恐怖が低いまま． |
| `future_assessment_update` | Decision | 窓 `W` の局所支持率トレンドを未来気候 `π_fut` へ外挿 (H4/H5)． |
| `voice_decision` | Decision | 中核の決定．`VoiceOracle` が発言確率を返す．ルール版は §数式のロジット，LLM 版は内省．スナップショット → 一括書戻しの同期更新． |
| `silence_spiral` | Interaction | 準統計的器官．自陣営として発言している近傍の比から `π_now` を更新 (HK 機構を離散表出観察へ組み替えた版)． |
| `prefalse_cascade` | Interaction | `PerAgentThresholdContagionMechanism` (BinaryState + Neighbors + ActivationThreshold)．各沈黙者の近傍発言比がその個体の閾値 θ_i に達したら発言へ反転 — 低 θ_i のハードコアは飽和近傍を待たず動員される少数派動員カスケード． |
| `metrics_record` | Reward | 正準 + 論文固有指標． |
| `climate_quasi_stat` | PostStep | 見かけ支持率 `q̂` を集約し，局所支持率履歴を積み，`q̂` 安定で停止要求． |

## 発言の数式 (ルールモード)

```
P(発言 | i) = logit⁻¹( β_0 + β_b·|b_i| + β_u·u_m·sign(b_i)
                       − β_f·f_i·(1−α_a)
                       + β_π·2·((1−α)·π_now_i + α·π_fut_i − ½)
                       + β_θ·1[ρ_i > θ_i] + ハードコアボーナス(θ_i) )
```

`α ∈ (0.5,1]` は未来重み (H5: 未来気候が現在を支配)．`α_a` は恐怖を縮減する匿名係数 (オンライン匿名の反事実)．ハードコアボーナスは `1−θ_i` に急峻なので，低閾値 (動員度の高い) 個体のみが不利な気候に抗して発言を続ける．

## RNG ストリーム

単一 root シードから `derive_seed` で独立 ChaCha20 ストリームを派生する．ラベル `0` = 世界初期化 (私的意見・恐怖・閾値・網)，`1` = エンジン (scheduler とカスケードの活性化順)，`2` = 媒体シグナル予約．ルールモードはビット決定論的で，統合テストがバイト等価再実行を検証する．LLM 層は socsim のビット再現性の外側にあり，`temperature=0` + プロンプト → 応答キャッシュで擬似決定論化する．

## 参考文献

- Noelle-Neumann, E. (1974). The Spiral of Silence: A Theory of Public Opinion. *Journal of Communication*, 24(2), 43–51.
- Hegselmann, R., & Krause, U. (2002). Opinion Dynamics and Bounded Confidence. *JASSS* 5(3). (気候観察のアーキタイプ)
- Granovetter, M. (1978). Threshold Models of Collective Behavior. *AJS* 83(6). (カスケードのアーキタイプ)
- Kuran, T. (1995). *Private Truths, Public Lies*. Harvard University Press. (私的 / 公的二層)
- Chuang et al. (2024). Simulating Opinion Dynamics with Networks of LLM-based Agents. *Findings of NAACL*. (LLM 二層パターン)
