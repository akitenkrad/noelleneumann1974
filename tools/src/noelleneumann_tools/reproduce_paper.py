#!/usr/bin/env python3
"""reproduce_paper.py — Noelle-Neumann (1974) Table 1--5 アンカーの再現レポート．

Rust の `noelleneumann reproduce` が書き出す reproduction_report.json (アンカーごとの
observed / target / pass) を読み，論文 §5 の Allensbach 調査値 (列車テスト 36%/51%，
勝/負陣営 53% vs 28%，未来評価ギャップ，ハードコア境界) と照合した PASS/off-anchor
テーブルを表示する．`--run` を付けると先に Rust バイナリを実行して最新結果を作る．

Usage:
    noelleneumann-tools reproduce
    noelleneumann-tools reproduce --results-dir results/20260530_000000_reproduce
    noelleneumann-tools reproduce --run            # 先に cargo run -- reproduce
    noelleneumann-tools reproduce --json

Outputs (stdout): アンカーごとの PASS / OFF と paper 値・observed・target 帯．
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from socsim_tools.io import resolve_results_dir


def _run_binary(seed: int) -> None:
    """cargo run --release -- reproduce を実行して最新結果を生成する．"""
    cmd = [
        "cargo",
        "run",
        "--release",
        "--",
        "reproduce",
        "--seed",
        str(seed),
    ]
    print(f"$ {' '.join(cmd)}")
    subprocess.run(cmd, check=True)


def _load_report(results_dir: Path) -> dict:
    path = results_dir / "reproduction_report.json"
    if not path.exists():
        raise FileNotFoundError(
            f"reproduction_report.json が見つかりません: {path}\n"
            f"  先に `noelleneumann reproduce` を実行するか --run を付けてください．"
        )
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools reproduce",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--results-dir", "--results_dir", default=None)
    parser.add_argument("--run", action="store_true", help="先に Rust バイナリを実行する．")
    parser.add_argument("--seed", type=int, default=42, help="--run 時のシード基点．")
    parser.add_argument("--json", action="store_true", help="JSON 形式で出力する．")
    args = parser.parse_args(argv)

    if args.run:
        _run_binary(args.seed)

    results_dir = resolve_results_dir(args.results_dir)
    try:
        report = _load_report(results_dir)
    except FileNotFoundError as exc:
        print(f"エラー: {exc}", file=sys.stderr)
        return 1

    anchors = report.get("anchors", [])
    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0

    print("=" * 78)
    print("The Spiral of Silence — Table 1--5 アンカー再現レポート")
    print(f"  source: {results_dir}")
    print("=" * 78)
    n_pass = 0
    for a in anchors:
        hi = a["target_hi"]
        hi_str = "∞" if hi is None or hi == float("inf") or hi > 1e30 else f"{hi:.2f}"
        status = "PASS" if a["pass"] else "OFF "
        if a["pass"]:
            n_pass += 1
        print(
            f"[{status}] {a['name']:<44} "
            f"obs={a['observed']:.4f} "
            f"target=[{a['target_lo']:.2f},{hi_str}] "
            f"paper={a['paper_value']}"
        )
    print("-" * 78)
    print(f"{n_pass}/{len(anchors)} アンカーが in-band")
    print("(中核アンカー: majority_voice_ratio log-OR>0.8 (H1) / "
          "future_assessment_gap>0.4 (H5) / hardcore_survival>0.7)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
