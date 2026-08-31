#!/usr/bin/env python3
"""reproduce_paper.py — Noelle-Neumann (1974) Table 1--5 アンカーの再現レポート．

Rust の `noelleneumann reproduce` が書いた run ディレクトリを読み，帯照合の結果を
表にする．読むのは 3 つのファイルで，役割が違う:

- `events.jsonl` の `x.noelleneumann1974.anchor` — 本再現が置いた帯と PASS / off の判定
- `metrics.csv` の run スコープ行                — 観測値そのもの
- `reference.csv`                                — **論文が報告した値**と出典

帯 (0.30--0.42 など) は本再現が置いたもので，論文の数ではない．論文の数は
`reference.csv` にしか無く，出典 (`source`) が必ず付く — この 2 つを混ぜないために
ファイルが分かれている．

`--run` を付けると先に Rust バイナリを実行して最新結果を作る．

Usage:
    uv run noelleneumann-tools reproduce
    uv run noelleneumann-tools reproduce --results-dir "$(runvault path --experiment noelleneumann --latest --subcommand reproduce)"
    uv run noelleneumann-tools reproduce --run            # 先に cargo run -- reproduce
    uv run noelleneumann-tools reproduce --json

Outputs (stdout): アンカーごとの PASS / OFF と observed・帯・論文値．
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

import pandas as pd
from runvault.read import events_table, runvault_path, scope_metrics_from_csv

EXPERIMENT = "noelleneumann"
ANCHOR_EVENT = "x.noelleneumann1974.anchor"

# legacy の reproduction_report.json の `name` → 現在の指標名．
LEGACY_NAMES = {
    "voice_volume (Table 1: 36%)": "steady_voice_volume",
    "majority_voice (Table 2: 53%)": "final_majority_voice",
    "minority_voice (Table 2: 28%)": "final_minority_voice",
    "majority_voice_ratio (H1: log-OR>0.8)": "steady_majority_voice_ratio",
    "pi_now pro (Table 4: ~0.49)": "final_pi_now_pro",
    "pi_now con (Table 4: ~0.43)": "final_pi_now_con",
    "future_assessment_gap (H5: >0.4)": "steady_future_assessment_gap",
    "hardcore_survival (hardcore_frac=0.25: >0.7)": "boundary_hardcore_survival",
}


def _run_binary(seed: int) -> None:
    """cargo run --release -- reproduce を実行して最新結果を生成する．"""
    cmd = ["cargo", "run", "--release", "--", "reproduce", "--seed", str(seed)]
    print(f"$ {' '.join(cmd)}")
    subprocess.run(cmd, check=True)


def load_anchors(results_dir: str) -> list[dict]:
    """帯照合の行を返す．

    runvault の run は `events.jsonl` を持つ．runvault 以前の run には
    `reproduction_report.json` が残っているので，あればそちらを読む — そちらが正本
    だった時期の結果を読めなくする理由はない．
    """
    legacy = os.path.join(results_dir, "reproduction_report.json")
    if os.path.exists(legacy):
        with open(legacy, encoding="utf-8") as f:
            report = json.load(f)
        rows = []
        for a in report.get("anchors", []):
            hi = a["target_hi"]
            rows.append({
                "indicator": LEGACY_NAMES.get(a["name"], a["name"]),
                "observed": a["observed"],
                "target_lo": a["target_lo"],
                "target_hi": None if hi is None or hi > 1e30 else hi,
                "pass": a["pass"],
            })
        return rows

    df = events_table(results_dir, kind=ANCHOR_EVENT)
    return df[["indicator", "observed", "target_lo", "target_hi", "pass"]].to_dict("records")


def load_paper_values(results_dir: str) -> dict[str, tuple[float, str]]:
    """`reference.csv` の «論文が報告した値» を指標名で引けるようにする．

    論文値を持たないアンカー (本再現が置いた定性的な帯だけのもの) はここに現れない．
    runvault 以前の run には `reference.csv` が無いので空を返す．
    """
    path = os.path.join(results_dir, "reference.csv")
    if not os.path.exists(path):
        return {}
    df = pd.read_csv(path)
    return {str(r["name"]): (float(r["value"]), str(r["source"])) for _, r in df.iterrows()}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools reproduce",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--results-dir", "--results_dir", default=None,
        help=(
            "reproduce の run ディレクトリ．未指定時は runvault に最新の run を聞く "
            "(--experiment noelleneumann --subcommand reproduce)．"
        ),
    )
    parser.add_argument(
        "--results-root", "--results_root", default="results",
        help="--results-dir 未指定時に runvault が探す results ルート (default: results)",
    )
    parser.add_argument("--run", action="store_true", help="先に Rust バイナリを実行する．")
    parser.add_argument("--seed", type=int, default=42, help="--run 時のシード基点．")
    parser.add_argument("--json", action="store_true", help="JSON 形式で出力する．")
    args = parser.parse_args(argv)

    if args.run:
        _run_binary(args.seed)

    results_dir = args.results_dir
    if results_dir is None:
        results_dir = runvault_path(EXPERIMENT, args.results_root, subcommand="reproduce")
    if not Path(results_dir).exists():
        print(f"エラー: ディレクトリが存在しません: {results_dir}", file=sys.stderr)
        return 1

    try:
        anchors = load_anchors(results_dir)
    except FileNotFoundError as exc:
        print(f"エラー: {exc}", file=sys.stderr)
        return 1
    papers = load_paper_values(results_dir)

    if args.json:
        payload = {
            "source": str(results_dir),
            "anchors": anchors,
            "paper_values": {k: {"value": v, "source": s} for k, (v, s) in papers.items()},
        }
        print(json.dumps(payload, indent=2, ensure_ascii=False))
        return 0

    print("=" * 88)
    print("The Spiral of Silence — Table 1--5 アンカー再現レポート")
    print(f"  source: {results_dir}")
    print("=" * 88)
    n_pass = 0
    for a in anchors:
        hi = a["target_hi"]
        hi_str = "∞" if hi is None or (isinstance(hi, float) and pd.isna(hi)) else f"{float(hi):.2f}"
        status = "PASS" if a["pass"] else "OFF "
        if a["pass"]:
            n_pass += 1
        paper = papers.get(a["indicator"])
        paper_str = f"paper={paper[0]:.2f}" if paper else "paper=—"
        print(
            f"[{status}] {a['indicator']:<30} "
            f"obs={float(a['observed']):.4f} "
            f"target=[{float(a['target_lo']):.2f},{hi_str}] "
            f"{paper_str}"
        )
    print("-" * 88)
    print(f"{n_pass}/{len(anchors)} アンカーが in-band")
    print("(中核アンカー: steady_majority_voice_ratio log-OR>0.8 (H1) / "
          "steady_future_assessment_gap>0.4 (H5) / boundary_hardcore_survival>0.7)")
    if papers:
        print("-" * 88)
        print("論文の報告値 (reference.csv):")
        for name, (value, source) in papers.items():
            print(f"  {name:<30} {value:.2f}  ← {source}")
        print("帯 (target) は本再現が置いたもので，論文の数ではない．")

    # 観測値は metrics.csv の run スコープ行にも同じ値がある (整合の確認用)．
    # legacy の run は wide な metrics.csv (時間軸の列が `t`) を持ち run スコープ行が
    # 無いので，long 形式を前提にした読み出しを掛けない．
    metrics_path = os.path.join(results_dir, "metrics.csv")
    scoped = (
        scope_metrics_from_csv(metrics_path)
        if os.path.exists(os.path.join(results_dir, "run.json")) and os.path.exists(metrics_path)
        else {}
    )
    if "anchors_total" in scoped:
        print("-" * 88)
        print(f"metrics.csv の集計: anchors_passed={int(scoped['anchors_passed'])} / "
              f"anchors_total={int(scoped['anchors_total'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
