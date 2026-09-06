# Visualization

[English](visualization.md) | [日本語](visualization.ja.md)

The Python package `noelleneumann-tools` reads a [runvault](https://github.com/akitenkrad/rs-runvault) run directory and renders figures. Install once at the workspace root with `uv sync`, then run any subcommand.

With no `--results-dir`, the tools ask runvault which run to read (`runvault path --experiment noelleneumann --latest …`) instead of scanning `results/` for the newest-looking directory. Pass `--results-dir` to pick a specific run; a pre-runvault `results/<timestamp>/` directory still works there.

Figures are written **beside** the run, in `results/noelleneumann/figures/<run_slug>/`. `manifest.csv` is settled by `finish()`, so anything added to the run directory afterwards would not be in it.

## `visualize` — single-run figures

```bash
uv run noelleneumann-tools visualize
uv run noelleneumann-tools visualize --results-dir "$(runvault path --experiment noelleneumann --latest --subcommand run --standalone)" --output-dir out
```

Produces two PNGs:

- `spiral_trajectory.png` — the public-discourse trajectory: apparent support `q̂(t)` and voice volume on the left axis, and the perceived-minus-actual gap `q̂ − q` on the right axis. The gap opening up over time is the spiral-of-silence signature (H3).
- `camp_voice.png` — the train-test willingness to voice for the majority vs the minority camp at the final tick, with the paper's Table-2 anchors (53% / 28%) overlaid as dotted lines.

## `visualize-sweep` — sweep figures

```bash
uv run noelleneumann-tools visualize-sweep --sweep-dir "$(runvault path --experiment noelleneumann --latest --subcommand sweep)"
```

The one-row-per-trial table is rebuilt from the `terminal` events of the sweep parent's children (`subcommand=sweep-point`); error bars need the individual trials, not the per-condition averages. A pre-runvault `sweep_summary.csv` is read directly when present.

Produces two PNGs, averaging over the seed repetitions per condition:

- `boundary_map.png` — a heatmap of `majority_voice_ratio` (log-OR) over the `η_m × network_beta` grid: the boundary where a global spiral emerges. Higher media homogeneity tends to raise the log-OR.
- `alpha_phase.png` — `future_assessment_gap` as a function of the future weight `α` (with error bars), against the H5 anchor line at 0.4.

## `show-experiment-settings` — settings dump

```bash
uv run noelleneumann-tools show-experiment-settings
uv run noelleneumann-tools show-experiment-settings --json
```

Prints the run's conditions (`config.json`; runvault keeps them under `parameters`) as an aligned table. Which subcommand the run was is answered by `run.json`. An LLM run also shows its `llm` block (provider / model snapshot / temperature) and the run-scope call breakdown. A pre-runvault directory is read in its own shape, `llm_meta.json` included.

## `reproduce` — Table 1–5 report

```bash
uv run noelleneumann-tools reproduce                 # reads the latest reproduce run
uv run noelleneumann-tools reproduce --run --seed 42 # runs the Rust binary first
uv run noelleneumann-tools reproduce --json
```

Reads the three files the `reproduce` run wrote — the anchor events, the run-scope observations, and `reference.csv` — and prints a PASS / off-anchor table with the paper's Table 1–5 values beside it. The band shown is this replication's; the `paper=` column comes from `reference.csv` and is the paper's own number. See [Reproduction](reproduction.md).

## Output files

Everything below is inside one runvault run directory.

| File | Written by | Contents |
|------|-----------|----------|
| `artifacts/opinions.csv` | `run`, `reproduce` | long format: `t, agent_id, b, e, pi_now, pi_fut` |
| `metrics.csv` | all | runvault's long form `run_uid,step,step_unit,scope,name,value`. The stepped rows are the eight per-tick metrics (the old wide columns, same names); the rows without a step describe the whole run (`converged`, `final_tick`, the LLM call breakdown, the `reproduce` observations) |
| `config.json` | all | the run's conditions, under `parameters` |
| `events.jsonl` | all | `observation` / `terminal` per unit — one unit for `run` and `reproduce`, one per trial for a sweep child — plus `x.noelleneumann1974.anchor` for `reproduce` |
| `reference.csv` | `reproduce` | the values the paper reports, with their sources |
| `run.json` / `status.json` / `manifest.csv` | all | identity (including the `llm` block), outcome and duration, file digests |
