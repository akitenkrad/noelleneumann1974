# Architecture

[English](architecture.md) | [日本語](architecture.ja.md)

## Repository structure

```
noelleneumann1974/
├── simulation/                    # Rust crate `noelleneumann-spiral-simulation` (bin `noelleneumann`)
│   ├── src/
│   │   ├── main.rs                # clap CLI: run / sweep / reproduce
│   │   ├── config.rs              # Config, NetworkModel, DecisionMode, JSON serialization
│   │   ├── world.rs               # SpiralWorld (WorldState + Neighbors + BinaryState), Expression
│   │   ├── mechanisms.rs          # media_signal / issue_salience / fear_appraisal /
│   │   │                          #   future_assessment / voice_decision / silence_spiral /
│   │   │                          #   climate_quasi_stat + VoiceOracle trait + RuleOracle
│   │   ├── metrics.rs             # paper-specific metrics + Metrics CSV row
│   │   ├── simulation.rs          # init_world + run drivers (SimulationBuilder wiring)
│   │   ├── llm.rs                 # (feature `llm`) thin re-export shim over socsim-llm
│   │   ├── prompts.rs             # LLM introspection prompt + response parser
│   │   └── lib.rs                 # public API for tests / examples
│   ├── examples/mock_smoke.rs     # (feature `llm`) offline scripted-LLM smoke
│   └── tests/                     # integration_test.rs + llm_mock_test.rs
├── tools/src/noelleneumann_tools/ # Python package `noelleneumann-tools`
│   ├── cli.py / visualize.py / visualize_sweep.py
│   ├── show_experiment_settings.py / reproduce_paper.py
├── docs/                          # this documentation (bilingual)
└── results/                       # runtime output (gitignored)
```

## The socsim framework

The simulation builds on [socsim](https://github.com/akitenkrad/rs-social-simulation-tools), a Rust ABM toolkit. It is an opinion-dynamics-on-a-network model, so it depends on:

- `socsim-core` — `WorldState` / `Mechanism` / `SimClock` / `SimRng` / `derive_seed` and the capability traits (`Neighbors`, `BinaryState`, `ActivationThreshold`).
- `socsim-engine` — `SimulationBuilder`, the six-phase step loop, and `RandomActivationScheduler`.
- `socsim-net` — `SocialNetwork` (Watts–Strogatz default; Erdős–Rényi and Barabási–Albert selectable).
- `socsim-mechanisms` — `PerAgentThresholdContagionMechanism`, reused directly for the hardcore / preference-falsification cascade (each agent's own θ_i is read from the world via `ActivationThreshold`).
- `socsim-metrics` (features `core`, `network`) — canonical `stats` (mean / variance / shannon_entropy / hhi / distinct_clusters) and `network::cascade_size`.
- `socsim-results` — timestamped run directories, the `latest` symlink, and CSV/JSON writers.
- `socsim-llm` (features `live`) — only compiled with the optional `llm` Cargo feature, for the introspection ablation.

## The two-layer world

`SpiralWorld` keeps the theoretical core: a private opinion vector `b_priv` and an independent public-expression vector `e_pub`. The `silence_spiral` mechanism observes only `e_pub` of neighbours — private opinion is unobservable — which is what generates the divergence between perceived and actual support (the H3 phenomenon, akin to preference falsification).

The world also carries the per-agent perceived current/future majority of one's own side (`pi_now` / `pi_fut`), a window of local-support history for trend extrapolation (`voice_history`), the isolation-fear tendency (`fear`) and expression threshold (`voice_threshold`, whose low tail is the hardcore), plus the scalar media signal and homogeneity.

## Nine mechanisms across six phases

| Mechanism | Phase | Role |
|-----------|-------|------|
| `media_signal` | Environment | Updates the exogenous opinion signal `u_m(t)`; homogeneity `η_m` blends a steady opinion-creating signal with noise. |
| `issue_salience_update` | Environment | Carries issue salience `σ(t)` (lightweight; an extension point). |
| `fear_appraisal` | Decision | Updates isolation fear `f_i` from perceived climate; hardcore (low threshold) keep fear low. |
| `future_assessment_update` | Decision | Extrapolates the local-support trend over a window `W` into the future climate `π_fut` (H4/H5). |
| `voice_decision` | Decision | The core choice. A `VoiceOracle` returns a voice probability; the rule oracle uses the §formula logit, the LLM oracle introspects. Synchronous snapshot-then-batch update. |
| `silence_spiral` | Interaction | The quasi-statistical organ: updates `π_now` from the fraction of *voicing* neighbours on one's own side (a Hegselmann–Krause-style local average adapted to discrete public expression). |
| `prefalse_cascade` | Interaction | `PerAgentThresholdContagionMechanism` (BinaryState + Neighbors + ActivationThreshold), flipping each silent agent to voice when its voicing-neighbour ratio reaches its own threshold θ_i — low-θ hardcore mobilize without waiting for a saturated neighbourhood, the mobilized-minority cascade. |
| `metrics_record` | Reward | Canonical and paper-specific metrics. |
| `climate_quasi_stat` | PostStep | Aggregates apparent support `q̂`, accumulates the local-support history, and requests stop once `q̂` stabilizes. |

## The voice formula (rule mode)

```
P(voice | i) = logit⁻¹( β_0 + β_b·|b_i| + β_u·u_m·sign(b_i)
                        − β_f·f_i·(1−α_a)
                        + β_π·2·((1−α)·π_now_i + α·π_fut_i − ½)
                        + β_θ·1[ρ_i > θ_i] + hardcore_bonus(θ_i) )
```

`α ∈ (0.5,1]` is the future weight (H5: the future climate dominates the present). `α_a` is an anonymity coefficient that shrinks fear (online-anonymity counterfactual). The hardcore bonus is steep in `1−θ_i`, so only low-threshold (mobilized) agents keep voicing against an adverse climate.

## RNG streams

A single root seed derives independent ChaCha20 streams via `derive_seed`: label `0` for world initialization (private opinions, fear, thresholds, network), `1` for the engine (scheduler and the cascade's activation order), and `2` reserved for the media signal. Rule mode is bit-deterministic — an integration test asserts byte-identical reruns. The LLM layer sits outside socsim's bit-reproducibility and is made pseudo-deterministic by `temperature=0` plus a prompt→response cache.

## References

- Noelle-Neumann, E. (1974). The Spiral of Silence: A Theory of Public Opinion. *Journal of Communication*, 24(2), 43–51.
- Hegselmann, R., & Krause, U. (2002). Opinion Dynamics and Bounded Confidence. *JASSS* 5(3). (the climate-observation archetype)
- Granovetter, M. (1978). Threshold Models of Collective Behavior. *AJS* 83(6). (the cascade archetype)
- Kuran, T. (1995). *Private Truths, Public Lies*. Harvard University Press. (private/public two-layer)
- Chuang et al. (2024). Simulating Opinion Dynamics with Networks of LLM-based Agents. *Findings of NAACL*. (the LLM two-layer pattern)

---
*This file was generated by Claude Code.*
