#!/usr/bin/env python3
"""visualize_sweep.py — Noelle-Neumann (1974) スイープ結果の可視化．

(1) η_m × network_beta の境界マップ (majority_voice_ratio のヒートマップ)，
(2) α (未来重み) 効果の相図 (α → future_assessment_gap)，
を生成する．条件ごとに seed 反復を平均する．

1 行 1 試行の表は，スイープ親 run の子 (`subcommand=sweep-point`) の `events.jsonl`
から組み直す．誤差棒 (±1σ) を描くには条件ごとの平均ではなく個々の試行が要るので，
子 run の run スコープ集約ではなく終端イベントを読む．runvault 以前の
`sweep_summary.csv` もそのまま読める．

Usage:
    uv run noelleneumann-tools visualize-sweep
    uv run noelleneumann-tools visualize-sweep --sweep-dir "$(runvault path --experiment noelleneumann --latest --subcommand sweep)"

Outputs:
    output_dir/
    ├── boundary_map.png   ← η_m × β の majority_voice_ratio ヒートマップ
    └── alpha_phase.png    ← α → future_assessment_gap の相図
"""

from __future__ import annotations

import argparse
import os

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from runvault.read import figures_dir, runvault_path, sweep_events_table

EXPERIMENT = "noelleneumann"

# 条件を表す parameters のキー (子 run の config.json から列にする)．
CONDITION_KEYS = ["eta_m", "network_beta", "alpha", "network_k", "true_support"]

# 終端イベントの `steady_*` は定常状態 (後半平均) の値で，legacy の
# `sweep_summary.csv` の同名の列と同じ量である．図の側は素の名前で読む．
STEADY_PREFIX = "steady_"

try:
    plt.rcParams["font.family"] = "Hiragino Sans"
except Exception:  # pragma: no cover
    pass

COLOR_BG = "#FAFAF8"


def load_summary(sweep_dir: str) -> pd.DataFrame:
    """1 行 1 試行の表 (条件 5 列 + 定常状態の指標) を返す．

    runvault はこの表をディスクに持たないので，スイープ親の子 run の終端イベントから
    組み直す．`steady_*` の接頭辞は落として legacy の列名に揃える．

    runvault 以前のスイープには `sweep_summary.csv` が残っているので，あればそちらを
    読む．そちらが正本だった時期の結果を読めなくする理由はない．
    """
    legacy = os.path.join(sweep_dir, "sweep_summary.csv")
    if os.path.exists(legacy):
        return pd.read_csv(legacy)

    df = sweep_events_table(sweep_dir, CONDITION_KEYS, kind="terminal")
    df = df.rename(columns={c: c[len(STEADY_PREFIX):] for c in df.columns
                            if c.startswith(STEADY_PREFIX)})
    df["run"] = df["unit_id"].str.removeprefix("trial-").astype(int)
    df["converged"] = ~df["censored"]
    df["final_tick"] = df["t"]
    return df


def plot_boundary_map(df: pd.DataFrame, out_path: str) -> None:
    """η_m × network_beta の majority_voice_ratio (log-OR) ヒートマップ．"""
    grp = (
        df.groupby(["eta_m", "network_beta"])["majority_voice_ratio"]
        .mean()
        .reset_index()
    )
    etas = sorted(grp["eta_m"].unique())
    betas = sorted(grp["network_beta"].unique())
    mat = np.full((len(betas), len(etas)), np.nan)
    for _, row in grp.iterrows():
        i = betas.index(row["network_beta"])
        j = etas.index(row["eta_m"])
        mat[i, j] = row["majority_voice_ratio"]

    fig, ax = plt.subplots(figsize=(7, 5))
    fig.patch.set_facecolor(COLOR_BG)
    im = ax.imshow(mat, origin="lower", aspect="auto", cmap="viridis")
    ax.set_xticks(range(len(etas)))
    ax.set_xticklabels([f"{e:.2g}" for e in etas])
    ax.set_yticks(range(len(betas)))
    ax.set_yticklabels([f"{b:.2g}" for b in betas])
    ax.set_xlabel("media homogeneity η_m")
    ax.set_ylabel("rewiring β")
    ax.set_title("Global spiral boundary\nmajority_voice_ratio (log-OR)")
    fig.colorbar(im, ax=ax, label="log-OR")
    for i in range(len(betas)):
        for j in range(len(etas)):
            if not np.isnan(mat[i, j]):
                ax.text(j, i, f"{mat[i, j]:.1f}", ha="center", va="center", color="w", fontsize=8)
    fig.tight_layout()
    fig.savefig(out_path, dpi=130)
    plt.close(fig)


def plot_alpha_phase(df: pd.DataFrame, out_path: str) -> None:
    """α (未来重み) → future_assessment_gap の相図 (誤差棒つき)．"""
    grp = df.groupby("alpha")["future_assessment_gap"].agg(["mean", "std"]).reset_index()
    fig, ax = plt.subplots(figsize=(7, 5))
    fig.patch.set_facecolor(COLOR_BG)
    ax.set_facecolor(COLOR_BG)
    ax.errorbar(
        grp["alpha"],
        grp["mean"],
        yerr=grp["std"].fillna(0.0),
        marker="o",
        color="#534AB7",
        capsize=4,
        lw=2.0,
    )
    ax.axhline(0.4, color="#C0392B", ls=":", lw=1.0, alpha=0.7, label="H5 anchor 0.4")
    ax.set_xlabel("future weight α")
    ax.set_ylabel("future_assessment_gap")
    ax.set_title("Future-assessment dominance (H5)")
    ax.legend(loc="best")
    fig.tight_layout()
    fig.savefig(out_path, dpi=130)
    plt.close(fig)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools visualize-sweep",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--sweep-dir", "--sweep_dir", "--results-dir", "--results_dir",
        default=None,
        help=(
            "スイープ親 run のディレクトリ．未指定時は runvault に最新のスイープを聞く "
            "(--experiment noelleneumann --subcommand sweep)．"
        ),
    )
    parser.add_argument(
        "--results-root", "--results_root", default="results",
        help="--sweep-dir 未指定時に runvault が探す results ルート (default: results)",
    )
    parser.add_argument(
        "--output-dir", "--output_dir", default=None,
        help="図の保存先 (default: results/noelleneumann/figures/{run_slug})",
    )
    args = parser.parse_args(argv)

    sweep_dir = args.sweep_dir
    if sweep_dir is None:
        sweep_dir = runvault_path(EXPERIMENT, args.results_root, subcommand="sweep")

    output_dir = args.output_dir or figures_dir(sweep_dir)
    os.makedirs(output_dir, exist_ok=True)

    print("=== 「沈黙の螺旋」 スイープ可視化 ===")
    print(f"スイープ: {sweep_dir}")
    print(f"出力先:   {output_dir}")
    print("-------------------------------------------------")

    df = load_summary(sweep_dir)
    print(f"      条件 {len(df.groupby(CONDITION_KEYS))} × 試行 {df['run'].nunique()}")
    p1 = os.path.join(output_dir, "boundary_map.png")
    p2 = os.path.join(output_dir, "alpha_phase.png")
    plot_boundary_map(df, p1)
    plot_alpha_phase(df, p2)
    print(f"boundary map  → {p1}")
    print(f"alpha phase   → {p2}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
