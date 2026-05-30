"""noelleneumann-tools show-experiment-settings — 実行結果の設定表示．

results/{ts}/config.json (run) または results/{ts}_sweep/sweep_config.json (sweep)
を読み，実行時パラメータを整形表示する．存在すれば llm_meta.json の LLM 情報も併せて
表示する．`results/latest` も解決される．

Usage:
    noelleneumann-tools show-experiment-settings
    noelleneumann-tools show-experiment-settings --results-dir results/20260530_000000
    noelleneumann-tools show-experiment-settings --results-dir results/latest --json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from socsim_tools.io import load_run_metadata, resolve_results_dir
from socsim_tools.settings import render_run_config, render_run_metadata

# config キー → 表示ラベル (右コロン位置を揃えるため空白パディング済み)．
FIELD_LABELS = {
    "n": "エージェント数 N ",
    "true_support": "真の支持率 q     ",
    "network_model": "網モデル         ",
    "network_k": "平均次数 k       ",
    "network_beta": "再配線率 β       ",
    "eta_m": "媒体均質性 η_m   ",
    "alpha": "未来重み α       ",
    "beta_pi": "気候係数 β_π     ",
    "beta_fear": "恐怖係数 β_f     ",
    "alpha_a": "匿名係数 α_a     ",
    "hardcore_frac": "ハードコア比率   ",
    "t_max": "最大 tick T      ",
    "decision_mode": "決定モード       ",
    "seed": "シード           ",
}


def _find_config_file(results_dir: Path) -> tuple[Path, str]:
    run_cfg = results_dir / "config.json"
    sweep_cfg = results_dir / "sweep_config.json"
    if run_cfg.exists():
        return run_cfg, "run"
    if sweep_cfg.exists():
        return sweep_cfg, "sweep"
    raise FileNotFoundError(
        f"設定ファイルが見つかりません: {results_dir}\n"
        f"  期待されるファイル: config.json (run) または sweep_config.json (sweep)"
    )


def render_sweep_config(cfg: dict, source: Path) -> str:
    """sweep 設定テーブルを整形する (リスト項目を `, ` 連結する)．"""

    def join(key: str) -> str:
        return ", ".join(str(v) for v in cfg.get(key, []))

    lines: list[str] = []
    lines.append("=" * 70)
    lines.append("実行設定 (sweep)")
    lines.append("=" * 70)
    lines.append(f"設定ファイル: {source}")
    lines.append("-" * 70)
    lines.append(f"媒体均質性 η_m   : {join('eta_m_values')}")
    lines.append(f"再配線率 β       : {join('network_beta_values')}")
    lines.append(f"未来重み α       : {join('alpha_values')}")
    lines.append(f"平均次数 k       : {join('network_k_values')}")
    lines.append(f"真の支持率 q     : {join('true_support_values')}")
    lines.append(f"エージェント数 N : {cfg.get('n', '-')}")
    lines.append(f"試行数 runs      : {cfg.get('runs', '-')}")
    lines.append(f"ハードコア比率   : {cfg.get('hardcore_frac', '-')}")
    lines.append(f"最大 tick T      : {cfg.get('t_max', '-')}")
    lines.append(f"シード基点       : {cfg.get('seed', '-')}")
    lines.append("=" * 70)
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools show-experiment-settings",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--results-dir", "--results_dir", default="results/latest")
    parser.add_argument("--json", action="store_true", help="JSON 形式で出力する．")
    args = parser.parse_args(argv)

    results_dir = resolve_results_dir(args.results_dir)
    if not results_dir.exists():
        print(f"エラー: ディレクトリが存在しません: {results_dir}", file=sys.stderr)
        return 1

    try:
        cfg_path, kind = _find_config_file(results_dir)
    except FileNotFoundError as exc:
        print(f"エラー: {exc}", file=sys.stderr)
        return 1
    with cfg_path.open() as f:
        cfg = json.load(f)
    meta = load_run_metadata(results_dir)

    if args.json:
        payload = {"source": str(cfg_path), "kind": kind, "config": cfg, "run_metadata": meta}
        print(json.dumps(payload, indent=2, ensure_ascii=False))
    else:
        if kind == "run":
            print(render_run_config(cfg, cfg_path, FIELD_LABELS))
        else:
            print(render_sweep_config(cfg, cfg_path))
        if meta is not None:
            print(render_run_metadata(meta))
    return 0


if __name__ == "__main__":
    sys.exit(main())
