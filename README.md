<p align="center"><img src="docs/assets/hero.svg" width="100%"></p>

**English** | [日本語](README.ja.md)

# The Spiral of Silence — Noelle-Neumann (1974)

A reimplementation of Elisabeth Noelle-Neumann's *The Spiral of Silence: A Theory of Public Opinion* (*Journal of Communication* 24(2), 43–51) as an agent-based opinion-dynamics model. The theory's core is a **two-layer separation** between an agent's **private opinion** `b_i ∈ [-1,1]` (fixed absent shocks) and its **public expression** `e_i ∈ {voice-pro, voice-con, silence}`. Each agent runs a "quasi-statistical organ": it observes only its neighbours' *public* expression (never their private opinion) to perceive how it stands in the current and future climate of opinion, and — fearing social isolation more than being wrong — falls silent when it perceives its side as a shrinking minority. The voice/silence asymmetry feeds back through the network, so one camp grows louder while the other goes quiet: a self-reinforcing **spiral of silence**, and a divergence between *perceived* and *actual* support.

The model adds an exogenous **media signal** (the paper's claim that "the mass media have to be seen as creating public opinion"), a **future-assessment** mechanism (the H5 hypothesis that the future climate dominates the present in deciding whether to speak), and a **hardcore** boundary (a mobilized, low-threshold minority that refuses to be silenced). The simulation is written in Rust on the [socsim](https://github.com/akitenkrad/rs-social-simulation-tools) framework; the visualization and reproduction tools are in Python.

## Install & Quick start

```bash
# Build the Rust simulation (rule-based decision mode; no LLM dependency)
cargo build --release

# Run the socialism-scenario baseline (n=1000, true support q=0.37)
cargo run --release -- run \
    --n 1000 --true-support 0.37 --network-model watts-strogatz \
    --eta-m 0.5 --alpha 0.7 --hardcore-frac 0.05 --t-max 80 --seed 42

# Install the Python visualization tools (at the workspace root)
uv sync

# Visualize the most recent run (spiral trajectory + camp voicing)
uv run noelleneumann-tools visualize

# Reproduce the paper's Table 1–5 anchors
cargo run --release -- reproduce --seed 42
uv run noelleneumann-tools reproduce
```

The optional LLM-introspection ablation (decide voice/silence by asking a language model to introspect the fear of isolation) is gated behind a Cargo feature and is **off by default** — the default build needs no LLM backend:

```bash
OLLAMA_MODEL=llama3.1 cargo run --release --features llm -- run --decision-mode llm \
    --n 200 --t-max 40 --seed 42 --cache-path .llm_cache/spiral.json
```

The prompt→response cache is persisted to `--cache-path`, so an identical warm rerun replays from disk (100% cache-hit) instead of re-calling the model; see [CLI](docs/cli.md).

## Documentation

- [Use cases](docs/usecases.md) — what you can do with this project, with pointers to the rest of the docs.
- [CLI](docs/cli.md) — the Rust CLI: the `run`, `sweep`, and `reproduce` subcommands and their flags.
- [Visualization](docs/visualization.md) — the Python `noelleneumann-tools` and how to interpret the outputs.
- [Reproduction](docs/reproduction.md) — the Table 1–5 anchors and how the model maps survey cross-sections to ABM steady states.
- [Architecture](docs/architecture.md) — repository structure, the socsim framework, the nine mechanisms across six phases, the formulas, and references.

## License

MIT
