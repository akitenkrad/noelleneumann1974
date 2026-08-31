# CLI

[English](cli.md) | [日本語](cli.ja.md)

The Rust binary `noelleneumann` has three subcommands: `run`, `sweep`, and `reproduce`. Build with `cargo build --release` and invoke via `cargo run --release -- <subcommand> [flags]`.

One invocation is one [runvault](https://github.com/akitenkrad/rs-runvault) run. The binary does not name its own output directory: `--output-dir` is the *results root* (default `results/`), and runvault creates `results/noelleneumann/<subcommand>_<timestamp>_<config_hash>_<execution_hash>/` under it. Ask runvault for a run rather than guessing the newest directory:

```bash
runvault path --experiment noelleneumann --latest --subcommand run --standalone
runvault path --experiment noelleneumann --latest --subcommand sweep
runvault verify "$(runvault path --experiment noelleneumann --latest --subcommand run --standalone)" --deep
```

## `run` — a single condition

```bash
cargo run --release -- run \
    --n 1000 --true-support 0.37 \
    --network-model watts-strogatz --network-k 6 --network-beta 0.1 \
    --eta-m 0.5 --alpha 0.7 --beta-pi 3.5 --beta-fear 2.5 \
    --hardcore-frac 0.05 --t-max 80 --seed 42
```

| Flag | Default | Meaning |
|------|---------|---------|
| `--n` | 1000 | number of agents |
| `--true-support` | 0.37 | true pro support `q = #{b>0}/n` |
| `--network-model` | watts-strogatz | `watts-strogatz` / `erdos-renyi` / `barabasi-albert` |
| `--network-k` | 6 | mean degree `k` (BA uses `k/2` attachments; ER uses `k/(n-1)` as edge prob) |
| `--network-beta` | 0.1 | Watts–Strogatz rewiring `β` |
| `--eta-m` | 0.5 | media homogeneity `η_m ∈ [0,1]` |
| `--alpha` | 0.7 | future weight `α ∈ (0.5,1]` (H5) |
| `--beta-pi` | (model) | climate coefficient `β_π` |
| `--beta-fear` | (model) | fear coefficient `β_f` |
| `--alpha-a` | 0.0 | anonymity coefficient `α_a` (shrinks fear) |
| `--hardcore-frac` | 0.05 | fraction of low-threshold hardcore agents |
| `--t-max` | 80 | maximum ticks |
| `--decision-mode` | rule | `rule` (no LLM) or `llm` (requires `--features llm`) |
| `--llm-temperature` | 0.0 | LLM generation temperature (LLM mode only; `0.0` = pseudo-deterministic) |
| `--llm-seed` | 0 | LLM generation seed passed to the backend (LLM mode only) |
| `--cache-path` | `.llm_cache/cache.json` | persistent prompt→response cache file (LLM mode only; ignored in rule mode) |
| `--seed` | random | RNG root seed (materialized before the run, so a run started without one still records the seed it used) |
| `--output-dir` | `results` | the runvault results root |

`run` writes the opinion trajectory to `artifacts/opinions.csv` (long: `t, agent_id, b, e, pi_now, pi_fut`) inside the run directory, the per-tick metrics to `metrics.csv` (runvault's long form: `run_uid,step,step_unit,scope,name,value`), the conditions to `config.json`, and one `terminal` line plus its `observation` lines to `events.jsonl`. The console prints the steady-state (second-half-average) metrics including the H1 log-OR, the H5 future-assessment gap, and hardcore survival.

## `sweep` — parameter scan

```bash
cargo run --release -- sweep \
    --eta-m-values 0.0,0.25,0.5,0.75,1.0 \
    --network-beta-values 0.0,0.05,0.1,0.3 \
    --alpha-values 0.5,0.6,0.7,0.8 \
    --network-k-values 6 --true-support-values 0.37 \
    --runs 30 --n 1000 --t-max 80 --seed 42
```

The scan is the Cartesian product of all `*-values` lists, repeated `--runs` times with independent derived seeds.

A sweep is a **parent run plus one child run per grid point**. The parent (`--subcommand sweep`) holds the grid definition in its `config.json` and no per-condition metrics; it declares no `master_seed`, because it is driven by a list of seeds rather than one. Each child (`--subcommand sweep-point`) holds that one condition, writes one `terminal` line per trial to its `events.jsonl` — the row that `sweep_summary.csv` used to hold — and the run-scope averages over its trials to its `metrics.csv`. The trial values are not metrics: putting them there would make `(run_uid, step, scope, name)` repeat.

## `reproduce` — Table 1–5 anchors

```bash
cargo run --release -- reproduce --n 1000 --t-max 80 --seed 42
```

Runs the socialism scenario (`q=0.37`) plus a hardcore-boundary scenario (`hardcore_frac=0.25`) — both inside **one** run, because the comparison between them is defined within a single execution — compares the observed metrics against the §5 Allensbach-survey anchors, and prints a `PASS` / `OFF` table.

The three parts of that table are written to three different files, because they are three different kinds of thing:

| File | Holds |
|------|-------|
| `metrics.csv` (run scope) | the observed values, plus `anchors_passed` / `anchors_total` |
| `events.jsonl` (`x.noelleneumann1974.anchor`) | the band and the PASS / off verdict — **this replication's** qualitative anchors, not the paper's numbers |
| `reference.csv` | the values **the paper reports** (Table 1 / 2 / 4), each with the source it was read from |

The band (`0.30`–`0.42` and so on) is ours; the paper's figure (`0.36`) is in `reference.csv` with its source. Keeping them apart is the point: merged into one row, they stop being distinguishable. The H1 log-OR > 0.8, the H5 gap > 0.4 and hardcore survival > 0.7 are ours alone and have no reference row.

The Python `noelleneumann-tools reproduce` reads all three (see [Reproduction](reproduction.md)).

## LLM ablation (optional)

Built only with the `llm` Cargo feature; the default build has no LLM dependency.

```bash
OLLAMA_MODEL=llama3.1 OLLAMA_HOST=http://localhost:11434 \
cargo run --release --features llm -- run --decision-mode llm \
    --n 200 --t-max 40 --seed 42 --cache-path .llm_cache/spiral.json
```

LLM mode persists the prompt→response cache to `--cache-path` (default `.llm_cache/cache.json`; the parent directory is created automatically). Because generation runs at `temperature=0` with a fixed seed and every prompt→response pair is cached on disk, a **warm rerun with the same arguments replays from the cache**: a cold run populates the file by calling the backend, and an identical warm run answers every prompt from the cache (a 100% cache-hit, near-instant replay) instead of re-calling the model. The backend that actually answered goes into `run.json`'s `llm` block (`provider` / `model_snapshot` / `temperature`) — the client is built before the run starts, because only the side that built it knows those. What is only known afterwards, the call breakdown, goes to `metrics.csv` as the run-scope `llm_calls` / `llm_cache_hits` / `llm_cache_hit_rate`. Delete the cache file to force a cold rerun.

An offline scripted-LLM smoke (no network) is available as an example:

```bash
cargo run --release --features llm --example mock_smoke -- results
```

---
*This file was generated by Claude Code.*
