#!/usr/bin/env python3
"""visualize.py — Noelle-Neumann (1974) The Spiral of Silence 可視化スクリプト．

runvault の run ディレクトリから意見の軌跡 (`artifacts/opinions.csv`) と
メトリクス (`metrics.csv`) を読み，
(1) 螺旋軌跡図 (公的言説支持率 q̂ と発言量・知覚-真値乖離の時系列)，
(2) 陣営別 (多数派/少数派) の発言意欲 (列車テスト) 棒グラフ，
を生成する．

どの run を見るかは `--results-dir` を省略すれば runvault が答える
(`runvault path --experiment noelleneumann --latest --subcommand run --standalone`)．
`results/` を自分で走査して新しそうなディレクトリを当てにいくことはしない．

図は run ディレクトリの *隣* (`results/noelleneumann/figures/<run_slug>/`) に置く．
`manifest.csv` は `finish()` が確定させたもので，run が終わった後に足したものは
そこに載らないためである．

Usage:
    uv run noelleneumann-tools visualize
    uv run noelleneumann-tools visualize --results-dir "$(runvault path --experiment noelleneumann --latest --subcommand run --standalone)"
    uv run noelleneumann-tools visualize --output-dir out

Outputs:
    output_dir/
    ├── spiral_trajectory.png  ← q̂ / voice_volume / perceived_minus_actual の時系列
    └── camp_voice.png         ← 多数派 vs 少数派の最終発言意欲 (棒)
"""

from __future__ import annotations

import argparse
import os

import matplotlib.pyplot as plt
import pandas as pd
from runvault.read import artifacts_dir, figures_dir, metrics_wide, runvault_path

EXPERIMENT = "noelleneumann"

try:
    plt.rcParams["font.family"] = "Hiragino Sans"
except Exception:  # pragma: no cover - フォント未インストール環境用フォールバック
    pass

COLOR_BG = "#FAFAF8"
COLOR_Q = "#534AB7"
COLOR_VOICE = "#0F6E56"
COLOR_GAP = "#C0392B"
COLOR_MAJ = "#534AB7"
COLOR_MIN = "#C0392B"


def load_opinions(path: str) -> pd.DataFrame:
    if not os.path.exists(path):
        raise FileNotFoundError(f"opinions.csv が見つかりません: {path}")
    return pd.read_csv(path)


def load_metrics(path: str) -> pd.DataFrame:
    """ステップごとのメトリクスを 1 tick 1 行の表として読む．

    runvault の `metrics.csv` は long 形式なので `metrics_wide` で横に倒す．時間軸の
    列名は runvault では `step` だが，本モデルの表記は論文に合わせた `t` なので，
    こちら側の呼び名に揃えてから返す (legacy の wide な metrics.csv はもともと `t` 列を
    持つので何もしない)．
    """
    df = metrics_wide(path)
    if "step" in df.columns and "t" not in df.columns:
        df = df.rename(columns={"step": "t"})
    return df


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
    parser.add_argument(
        "--results-dir", "--results_dir", default=None,
        help=(
            "runvault の run ディレクトリ．未指定時は runvault に最新の run を聞く "
            "(--experiment noelleneumann --subcommand run --standalone)．"
        ),
    )
    parser.add_argument(
        "--results-root", "--results_root", default="results",
        help="--results-dir 未指定時に runvault が探す results ルート (default: results)",
    )
    parser.add_argument(
        "--output-dir", "--output_dir", default=None,
        help="図の保存先 (default: results/noelleneumann/figures/{run_slug})",
    )
    args = parser.parse_args(argv)

    run_dir = args.results_dir
    if run_dir is None:
        run_dir = runvault_path(
            EXPERIMENT, args.results_root, subcommand="run", standalone=True
        )

    opinions_path = os.path.join(artifacts_dir(run_dir), "opinions.csv")
    metrics_path = os.path.join(run_dir, "metrics.csv")
    output_dir = args.output_dir or figures_dir(run_dir)
    os.makedirs(output_dir, exist_ok=True)

    print("=== 「沈黙の螺旋」 可視化 ===")
    print(f"run:        {run_dir}")
    print(f"出力先:     {output_dir}")
    print("-----------------------------------------")

    opinions = load_opinions(opinions_path)
    metrics = load_metrics(metrics_path)
    p1 = os.path.join(output_dir, "spiral_trajectory.png")
    p2 = os.path.join(output_dir, "camp_voice.png")
    plot_spiral_trajectory(metrics, p1)
    plot_camp_voice(opinions, p2)
    print(f"spiral trajectory  → {p1}")
    print(f"camp voicing       → {p2}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
