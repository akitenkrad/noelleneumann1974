# 可視化

[English](visualization.md) | [日本語](visualization.ja.md)

Python パッケージ `noelleneumann-tools` は [runvault](https://github.com/akitenkrad/rs-runvault) の run ディレクトリを読んで図を描く．workspace ルートで一度 `uv sync` し，各サブコマンドを実行する．

`--results-dir` 省略時は，`results/` を走査して新しそうなディレクトリを当てにいくのではなく runvault に聞く (`runvault path --experiment noelleneumann --latest …`)．特定の run を見たいときは `--results-dir` に渡す．runvault 以前の `results/<timestamp>/` もそのまま渡せる．

図は run ディレクトリの **隣** (`results/noelleneumann/figures/<run_slug>/`) に置く．`manifest.csv` は `finish()` が確定させたもので，run が終わった後に足したものはそこに載らないためである．

## `visualize` — 単一実行の図

```bash
uv run noelleneumann-tools visualize
uv run noelleneumann-tools visualize --results-dir "$(runvault path --experiment noelleneumann --latest --subcommand run --standalone)" --output-dir out
```

2 つの PNG を生成する．

- `spiral_trajectory.png` — 公的言説の軌跡．左軸に見かけ支持率 `q̂(t)` と発言量，右軸に知覚-真値ギャップ `q̂ − q`．時間とともにギャップが開くのが沈黙の螺旋の signature (H3)．
- `camp_voice.png` — 最終 tick における多数派 vs 少数派の列車テスト発言意欲．論文 Table 2 のアンカー (53% / 28%) を点線で重畳する．

## `visualize-sweep` — スイープの図

```bash
uv run noelleneumann-tools visualize-sweep --sweep-dir "$(runvault path --experiment noelleneumann --latest --subcommand sweep)"
```

1 行 1 試行の表は，スイープ親 run の子 (`subcommand=sweep-point`) の `terminal` イベントから組み直す — 誤差棒には条件ごとの平均ではなく個々の試行が要る．runvault 以前の `sweep_summary.csv` があればそちらを読む．

条件ごとに seed 反復を平均し，2 つの PNG を生成する．

- `boundary_map.png` — `η_m × network_beta` 格子上の `majority_voice_ratio` (log-OR) ヒートマップ．大域的螺旋が出現する境界．媒体均質性が高いほど log-OR が上がる傾向．
- `alpha_phase.png` — 未来重み `α` に対する `future_assessment_gap` (誤差棒つき)．H5 アンカー線 (0.4) と対比する．

## `show-experiment-settings` — 設定表示

```bash
uv run noelleneumann-tools show-experiment-settings
uv run noelleneumann-tools show-experiment-settings --json
```

run の条件 (`config.json`．runvault では `parameters` の下) を桁揃えした表で表示する．どのサブコマンドの run かは `run.json` が答える．LLM の run では `llm` ブロック (provider / model snapshot / 温度) と run スコープの呼び出し内訳も併せて表示する．runvault 以前のディレクトリは当時の形のまま (`llm_meta.json` を含めて) 読む．

## `reproduce` — Table 1--5 レポート

```bash
uv run noelleneumann-tools reproduce                 # 最新の reproduce run を読む
uv run noelleneumann-tools reproduce --run --seed 42 # 先に Rust バイナリを実行する
uv run noelleneumann-tools reproduce --json
```

`reproduce` の run が書いた 3 つ — アンカーのイベント，run スコープの観測値，`reference.csv` — を読み，論文 Table 1--5 値を併記した PASS / off-anchor テーブルを表示する．表示される帯は本再現のもの，`paper=` の列は `reference.csv` にある論文自身の数である．[再現](reproduction.ja.md) 参照．

## 出力ファイル

以下はすべて runvault の run ディレクトリ 1 本の中身である．

| ファイル | 書き出し元 | 内容 |
|------|-----------|------|
| `artifacts/opinions.csv` | `run`, `reproduce` | long 形式: `t, agent_id, b, e, pi_now, pi_fut` |
| `metrics.csv` | 全部 | runvault の long 形式 `run_uid,step,step_unit,scope,name,value`．step を持つ行が tick ごとの 8 指標 (旧 wide の列そのまま，名前も同じ)，step を持たない行が run 全体を 1 つの値で表すもの (`converged` / `final_tick` / LLM 呼び出しの内訳 / `reproduce` の観測量) |
| `config.json` | 全部 | run の条件 (`parameters` の下) |
| `events.jsonl` | 全部 | 単位ごとの `observation` / `terminal` (`run` と `reproduce` は 1 単位，sweep の子は試行ごと) と，`reproduce` の `x.noelleneumann1974.anchor` |
| `reference.csv` | `reproduce` | 論文が報告した値と出典 |
| `run.json` / `status.json` / `manifest.csv` | 全部 | 同一性 (`llm` ブロックを含む)・結果と実行時間・ファイルのダイジェスト |
