#!/usr/bin/env python3
"""visualize.py — Noelle-Neumann (1974) The Spiral of Silence 可視化スクリプト．

results/latest (または --results-dir 指定先) の opinions.csv / metrics.csv を読み，
(1) 螺旋軌跡図 (公的言説支持率 q̂ と発言量・知覚-真値乖離の時系列)，
(2) 陣営別 (多数派/少数派) の発言意欲 (列車テスト) 棒グラフ，
を生成する．

Usage:
    noelleneumann-tools visualize
    noelleneumann-tools visualize --results-dir results/20260530_000000
    noelleneumann-tools visualize --output-dir out

Outputs:
    output_dir/
    ├── spiral_trajectory.png  ← q̂ / voice_volume / perceived_minus_actual の時系列
    └── camp_voice.png         ← 多数派 vs 少数派の最終発言意欲 (棒)
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
COLOR_Q = "#534AB7"
COLOR_VOICE = "#0F6E56"
COLOR_GAP = "#C0392B"
COLOR_MAJ = "#534AB7"
COLOR_MIN = "#C0392B"


def _load(results_dir: str) -> tuple[pd.DataFrame, pd.DataFrame]:
    opath = os.path.join(results_dir, "opinions.csv")
    mpath = os.path.join(results_dir, "metrics.csv")
    if not os.path.exists(opath):
        raise FileNotFoundError(f"opinions.csv が見つかりません: {opath}")
    if not os.path.exists(mpath):
        raise FileNotFoundError(f"metrics.csv が見つかりません: {mpath}")
    return pd.read_csv(opath), pd.read_csv(mpath)


def plot_spiral_trajectory(metrics: pd.DataFrame, out_path: str) -> None:
    """公的支持率 q̂・発言量・知覚-真値乖離の時系列 (螺旋の軌跡)．"""
    fig, ax1 = plt.subplots(figsize=(9, 5))
    fig.patch.set_facecolor(COLOR_BG)
    ax1.set_facecolor(COLOR_BG)
    t = metrics["t"]
    ax1.plot(t, metrics["apparent_support"], color=COLOR_Q, lw=2.0, label="apparent support q̂")
    ax1.plot(t, metrics["voice_volume"], color=COLOR_VOICE, lw=2.0, label="voice volume")
    ax1.set_xlabel("time t")
    ax1.set_ylabel("share")
    ax1.set_ylim(-0.05, 1.05)

    ax2 = ax1.twinx()
    ax2.plot(
        t,
        metrics["perceived_minus_actual"],
        color=COLOR_GAP,
        lw=1.6,
        ls="--",
        label="perceived − actual (q̂ − q)",
    )
    ax2.set_ylabel("perceived − actual")

    lines1, labels1 = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    ax1.legend(lines1 + lines2, labels1 + labels2, loc="best", framealpha=0.9)
    ax1.set_title("Spiral of Silence — public discourse trajectory")
    fig.tight_layout()
    fig.savefig(out_path, dpi=130)
    plt.close(fig)


def plot_camp_voice(opinions: pd.DataFrame, out_path: str) -> None:
    """最終 tick の陣営別 (多数派 vs 少数派) 発言意欲 (列車テスト) 棒グラフ．

    opinions.csv の最終 tick で b の符号により多数派/少数派を分け，自陣営として発言
    (e=+1 は b>0 の pro，e=-1 は b<0 の con) している比率を出す．
    """
    last_t = opinions["t"].max()
    final = opinions[opinions["t"] == last_t]
    pro = final[final["b"] > 0]
    con = final[final["b"] < 0]
    # 多数派 = 人数の多い側．
    if len(pro) >= len(con):
        maj, mino = pro, con
        maj_label, min_label = "majority (pro)", "minority (con)"
        maj_voice = (maj["e"] == 1).mean() if len(maj) else 0.0
        min_voice = (mino["e"] == -1).mean() if len(mino) else 0.0
    else:
        maj, mino = con, pro
        maj_label, min_label = "majority (con)", "minority (pro)"
        maj_voice = (maj["e"] == -1).mean() if len(maj) else 0.0
        min_voice = (mino["e"] == 1).mean() if len(mino) else 0.0

    fig, ax = plt.subplots(figsize=(6, 5))
    fig.patch.set_facecolor(COLOR_BG)
    ax.set_facecolor(COLOR_BG)
    bars = ax.bar(
        [maj_label, min_label],
        [maj_voice, min_voice],
        color=[COLOR_MAJ, COLOR_MIN],
        width=0.55,
    )
    for b, v in zip(bars, [maj_voice, min_voice]):
        ax.text(b.get_x() + b.get_width() / 2, v + 0.02, f"{v:.2f}", ha="center", fontsize=11)
    # 論文 Table 2 のアンカー (53% / 28%) を点線で重ねる．
    ax.axhline(0.53, color=COLOR_MAJ, ls=":", lw=1.0, alpha=0.6)
    ax.axhline(0.28, color=COLOR_MIN, ls=":", lw=1.0, alpha=0.6)
    ax.set_ylim(0, 1.05)
    ax.set_ylabel("willingness to voice (train-test)")
    ax.set_title("Train-test voicing by camp\n(dotted: paper anchors 53% / 28%)")
    fig.tight_layout()
    fig.savefig(out_path, dpi=130)
    plt.close(fig)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools visualize",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--results-dir", "--results_dir", default=None)
    parser.add_argument("--output-dir", "--output_dir", default=None)
    args = parser.parse_args(argv)

    results_dir = str(resolve_results_dir(args.results_dir))
    output_dir = args.output_dir or results_dir
    os.makedirs(output_dir, exist_ok=True)

    opinions, metrics = _load(results_dir)
    p1 = os.path.join(output_dir, "spiral_trajectory.png")
    p2 = os.path.join(output_dir, "camp_voice.png")
    plot_spiral_trajectory(metrics, p1)
    plot_camp_voice(opinions, p2)
    print(f"spiral trajectory  → {p1}")
    print(f"camp voicing       → {p2}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
