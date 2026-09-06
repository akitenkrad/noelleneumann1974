# CLI

[English](cli.md) | [日本語](cli.ja.md)

Rust バイナリ `noelleneumann` は `run` / `sweep` / `reproduce` の 3 サブコマンドを持つ．`cargo build --release` でビルドし，`cargo run --release -- <subcommand> [flags]` で実行する．

## `run` — 単一条件

```bash
cargo run --release -- run \
    --n 1000 --true-support 0.37 \
    --network-model watts-strogatz --network-k 6 --network-beta 0.1 \
    --eta-m 0.5 --alpha 0.7 --beta-pi 3.5 --beta-fear 2.5 \
    --hardcore-frac 0.05 --t-max 80 --seed 42
```

| フラグ | 既定 | 意味 |
|------|------|------|
| `--n` | 1000 | エージェント数 |
| `--true-support` | 0.37 | 真の賛成支持率 `q = #{b>0}/n` |
| `--network-model` | watts-strogatz | `watts-strogatz` / `erdos-renyi` / `barabasi-albert` |
| `--network-k` | 6 | 平均次数 `k` (BA は `k/2` 接続，ER は `k/(n-1)` を辺確率に換算) |
| `--network-beta` | 0.1 | Watts--Strogatz 再配線率 `β` |
| `--eta-m` | 0.5 | 媒体均質性 `η_m ∈ [0,1]` |
| `--alpha` | 0.7 | 未来重み `α ∈ (0.5,1]` (H5) |
| `--beta-pi` | (モデル) | 気候係数 `β_π` |
| `--beta-fear` | (モデル) | 恐怖係数 `β_f` |
| `--alpha-a` | 0.0 | 匿名係数 `α_a` (恐怖を縮減) |
| `--hardcore-frac` | 0.05 | 低閾値ハードコアの比率 |
| `--t-max` | 80 | 最大 tick 数 |
| `--decision-mode` | rule | `rule` (LLM 非呼び出し) / `llm` (`--features llm` 必須) |
| `--llm-temperature` | 0.0 | LLM 生成温度 (llm モードのみ．`0.0` = 擬似決定論) |
| `--llm-seed` | 0 | バックエンドへ渡す LLM 生成シード (llm モードのみ) |
| `--cache-path` | `.llm_cache/cache.json` | プロンプト→応答キャッシュの永続ファイル (llm モードのみ．rule モードは無視) |
| `--seed` | random | RNG root シード (run 前に実体化するので，省略した実行でも «実際に使われたシード» が残る) |
| `--output-dir` | `results` | runvault の results ルート |

`run` は意見の軌跡を run ディレクトリの `artifacts/opinions.csv` (long: `t, agent_id, b, e, pi_now, pi_fut`) に，tick ごとの指標を `metrics.csv` (runvault の long 形式 `run_uid,step,step_unit,scope,name,value`) に，条件を `config.json` に，`terminal` 行 1 本とその `observation` 行を `events.jsonl` に書く．コンソールには定常状態 (後半平均) 指標 (H1 log-OR，H5 未来評価ギャップ，ハードコア生存率) を表示する．

## `sweep` — パラメータ走査

```bash
cargo run --release -- sweep \
    --eta-m-values 0.0,0.25,0.5,0.75,1.0 \
    --network-beta-values 0.0,0.05,0.1,0.3 \
    --alpha-values 0.5,0.6,0.7,0.8 \
    --network-k-values 6 --true-support-values 0.37 \
    --runs 30 --n 1000 --t-max 80 --seed 42
```

走査は全 `*-values` リストの直積を `--runs` 回 (独立派生シードで) 反復する．

スイープは **親 run 1 本 + 格子点ごとの子 run** になる．親 (`--subcommand sweep`) は走査グリッドの定義を `config.json` に持ち，条件ごとの指標は書かない．シードの列で駆動されるので `master_seed` は名乗らない．子 (`--subcommand sweep-point`) は格子点 1 つを持ち，試行ごとの値 — 旧 `sweep_summary.csv` の 1 行にあたるもの — を `events.jsonl` の `terminal` 行として，試行群の平均を `metrics.csv` の run スコープ指標として書く．試行ごとの値を指標にすると `(run_uid, step, scope, name)` が重複するので `metrics.csv` には入れない．

## `reproduce` — Table 1--5 アンカー

```bash
cargo run --release -- reproduce --n 1000 --t-max 80 --seed 42
```

社会主義シナリオ (`q=0.37`) とハードコア境界シナリオ (`hardcore_frac=0.25`) を **1 本の run の中で**実行し (両者の比較が 1 回の実行の中で定義されているため子 run には分けない)，観測指標を §5 のアレンスバッハ調査アンカーと照合して `PASS` / `OFF` テーブルを表示する．

このテーブルの 3 要素は種類が違うので，書き先も 3 つに分かれる:

| ファイル | 持つもの |
|---------|---------|
| `metrics.csv` (run スコープ) | 観測値そのものと `anchors_passed` / `anchors_total` |
| `events.jsonl` (`x.noelleneumann1974.anchor`) | 帯と PASS / off の判定 — **本再現が置いた**定性的なアンカーで，論文の数ではない |
| `reference.csv` | **論文が報告した値** (Table 1 / 2 / 4) と，その出典 |

帯 (`0.30`--`0.42` など) はこちらのもの，論文の数 (`0.36`) は出典付きで `reference.csv` にある．分けておくことに意味がある — 1 行に混ぜた時点で，どちらが論文の数なのかが見分けられなくなる．H1 の log-OR > 0.8，H5 の gap > 0.4，ハードコア生存 > 0.7 はこちらのものだけなので reference 行を持たない．

Python の `noelleneumann-tools reproduce` はこの 3 つを読む ([再現](reproduction.ja.md) 参照)．

## LLM ablation (任意)

`llm` Cargo feature でのみビルドされる．既定ビルドは LLM 依存なし．

```bash
OLLAMA_MODEL=llama3.1 OLLAMA_HOST=http://localhost:11434 \
cargo run --release --features llm -- run --decision-mode llm \
    --n 200 --t-max 40 --seed 42 --cache-path .llm_cache/spiral.json
```

LLM モードはプロンプト→応答キャッシュを `--cache-path` (既定 `.llm_cache/cache.json`．親ディレクトリは自動作成) に永続化する．生成は `temperature=0` + 固定シードで走り，全プロンプト→応答対をディスクにキャッシュするため，**同一引数の温暖な再実行はキャッシュから再生される**: cold run はバックエンドを呼んでファイルを満たし，同一の warm run は全プロンプトをキャッシュから応答する (100% cache-hit のほぼ即時再生) ためモデルを再呼び出ししない．実際に応答したバックエンドは `run.json` の `llm` ブロック (`provider` / `model_snapshot` / `temperature`) に入る — それを知っているのはクライアントを組んだ側だけなので，クライアントは run を開始する前に組む．実行しないと分からない呼び出しの内訳は `metrics.csv` の run スコープ指標 `llm_calls` / `llm_cache_hits` / `llm_cache_hit_rate` に入る．cold で再実行したい場合はキャッシュファイルを削除する．

オフライン (網不要) の scripted-LLM スモークは例として用意している．

```bash
cargo run --release --features llm --example mock_smoke -- results
```
