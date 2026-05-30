#!/usr/bin/env python3
"""visualize_sweep.py — Noelle-Neumann (1974) スイープ結果の可視化．

results/{ts}_sweep/sweep_summary.csv を読み，
(1) η_m × network_beta の境界マップ (majority_voice_ratio のヒートマップ)，
(2) α (未来重み) 効果の相図 (α → future_assessment_gap)，
を生成する．条件ごとに seed 反復を平均する．

Usage:
    noelleneumann-tools visualize-sweep
    noelleneumann-tools visualize-sweep --results-dir results/20260530_000000_sweep

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
from socsim_tools.io import resolve_results_dir

plt.rcParams["font.family"] = "Hiragino Sans"
COLOR_BG = "#FAFAF8"


def _load(results_dir: str) -> pd.DataFrame:
    path = os.path.join(results_dir, "sweep_summary.csv")
    if not os.path.exists(path):
        raise FileNotFoundError(f"sweep_summary.csv が見つかりません: {path}")
    return pd.read_csv(path)


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
    parser.add_argument("--results-dir", "--results_dir", default=None)
    parser.add_argument("--output-dir", "--output_dir", default=None)
    args = parser.parse_args(argv)

    results_dir = str(resolve_results_dir(args.results_dir))
    output_dir = args.output_dir or results_dir
    os.makedirs(output_dir, exist_ok=True)

    df = _load(results_dir)
    p1 = os.path.join(output_dir, "boundary_map.png")
    p2 = os.path.join(output_dir, "alpha_phase.png")
    plot_boundary_map(df, p1)
    plot_alpha_phase(df, p2)
    print(f"boundary map  → {p1}")
    print(f"alpha phase   → {p2}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
