"""noelleneumann-tools show-experiment-settings — 実行結果の設定表示．

runvault の run ディレクトリの `config.json` (封筒．条件は `parameters` の下) を読み，
実行時に使われた全パラメータを整形表示する．run / sweep / sweep-point / reproduce の
どれかは `run.json` の `subcommand` が答える．LLM モードの run は `run.json` の `llm`
ブロックと run スコープ指標 (呼び出し回数・cache-hit) も併せて表示する．
legacy の flat な `config.json` / `sweep_config.json` も読める．

run ディレクトリのパスは次で取れる:
    runvault path --experiment noelleneumann --latest --subcommand run --standalone
    runvault path --experiment noelleneumann --latest --subcommand sweep

Usage:
    noelleneumann-tools show-experiment-settings
    noelleneumann-tools show-experiment-settings --results-dir "$(runvault path --experiment noelleneumann --latest --subcommand run --standalone)"
    noelleneumann-tools show-experiment-settings --json
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

from runvault.read import config_parameters, load_run_meta, run_scope_metrics, runvault_path

EXPERIMENT = "noelleneumann"

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
    "runs": "試行数 runs      ",
    "boundary_hardcore_frac": "境界シナリオ比率 ",
}


def resolve_results_dir(path_like: str) -> Path:
    """シンボリックリンク (legacy の `results/latest`) を実体に解決する．"""
    p = Path(path_like)
    if p.is_symlink():
        return Path(os.path.realpath(p))
    return p


def _load_config(results_dir: Path) -> tuple[dict, Path, str]:
    """run ディレクトリの実験条件と，それがどのサブコマンドのものかを返す．

    runvault の `config.json` は封筒で，条件は `parameters` の下にある．どの
    サブコマンドかは `run.json` が答える (`sweep_config.json` はもう書かれない)．
    """
    # 設定が無いことは «まだ sweep_config.json の方かもしれない» という意味なので，
    # ここでは欠落を失敗として扱わない (下で sweep_config.json を見る)．
    params = config_parameters(results_dir, required=False)
    if params is not None:
        meta = load_run_meta(results_dir, required=False)
        if meta is not None:
            kind = str(meta.get("subcommand", "run"))
        else:
            # legacy: 自前で書いていた config.json は "command" を持つ
            kind = "sweep" if params.get("command") == "sweep" else "run"
        return params, results_dir / "config.json", kind

    sweep_cfg = results_dir / "sweep_config.json"
    if sweep_cfg.exists():
        with sweep_cfg.open() as f:
            return json.load(f), sweep_cfg, "sweep"

    raise FileNotFoundError(
        f"設定ファイルが見つかりません: {results_dir}\n"
        f"  期待されるファイル: config.json (runvault の封筒 / legacy の flat) "
        f"または sweep_config.json (legacy の sweep)"
    )


def render_flat_config(cfg: dict, source: Path, kind: str) -> str:
    """1 条件ぶんの設定テーブル (run / sweep-point / reproduce)．"""
    lines: list[str] = []
    lines.append("=" * 70)
    lines.append(f"実行設定 ({kind})")
    lines.append("=" * 70)
    lines.append(f"設定ファイル: {source}")
    lines.append("-" * 70)
    for key, label in FIELD_LABELS.items():
        if key in cfg:
            lines.append(f"{label}: {cfg[key]}")
    # モデル定数 (CLI 非露出) も記録されているので併せて出す．
    consts = [k for k in ("beta_0", "beta_b", "beta_u", "beta_theta", "lambda", "gamma", "window")
              if k in cfg]
    if consts:
        lines.append("-" * 70)
        lines.append("モデル定数: " + ", ".join(f"{k}={cfg[k]}" for k in consts))
    # 出力先は run ディレクトリそのものなので条件には含まれない (legacy のみ持つ)．
    if cfg.get("output_dir") is not None:
        lines.append(f"出力先           : {cfg['output_dir']}")
    lines.append("=" * 70)
    return "\n".join(lines)


def render_sweep_config(cfg: dict, source: Path) -> str:
    """sweep 親の設定テーブル (リスト項目を `, ` 連結する)．"""

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


def load_legacy_llm_meta(results_dir: Path) -> dict | None:
    """runvault 以前の run が持つ `llm_meta.json`．

    現在は model / endpoint / temperature が `run.json` の `llm` ブロックに，
    calls / cache_hits / cache_hit_rate が run スコープ指標に分かれて入るが，
    その前の run はこの 1 ファイルに全部持っている．
    """
    path = results_dir / "llm_meta.json"
    if not path.exists():
        return None
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def render_llm(meta: dict, scoped: dict, legacy: dict | None) -> str | None:
    """`run.json` の llm ブロックと run スコープの呼び出し内訳．

    どれも無い run (rule モード) では何も出さない．
    """
    llm = meta.get("llm")
    if llm is None and "llm_calls" not in scoped and legacy is None:
        return None
    lines: list[str] = []
    lines.append("-" * 70)
    if legacy is not None:
        lines.append("LLM (legacy の llm_meta.json)")
        for key in ("model", "endpoint", "backend", "temperature", "seed"):
            if key in legacy:
                lines.append(f"{key:<16} : {legacy[key]}")
        if "calls" in legacy:
            lines.append(f"呼び出し回数     : {legacy['calls']}")
            lines.append(f"cache-hit        : {legacy.get('cache_hits', 0)}"
                         f" ({legacy.get('cache_hit_rate', 0.0) * 100:.1f}%)")
        lines.append("=" * 70)
        return "\n".join(lines)
    lines.append("LLM (run.json の llm ブロック / run スコープ指標)")
    if llm is not None:
        lines.append(f"provider         : {llm.get('provider', '-')}")
        lines.append(f"model_snapshot   : {llm.get('model_snapshot', '-')}")
        lines.append(f"temperature      : {llm.get('temperature', '-')}")
    if "llm_calls" in scoped:
        lines.append(f"呼び出し回数     : {int(scoped['llm_calls'])}")
        lines.append(f"cache-hit        : {int(scoped.get('llm_cache_hits', 0))}"
                     f" ({scoped.get('llm_cache_hit_rate', 0.0) * 100:.1f}%)")
    lines.append("=" * 70)
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="noelleneumann-tools show-experiment-settings",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--results-dir", "--results_dir", default=None,
        help=(
            "run ディレクトリ．未指定時は runvault に最新の run を聞く "
            "(--experiment noelleneumann --subcommand run --standalone)．"
        ),
    )
    parser.add_argument(
        "--results-root", "--results_root", default="results",
        help="--results-dir 未指定時に runvault が探す results ルート (default: results)",
    )
    parser.add_argument("--json", action="store_true", help="JSON 形式で出力する．")
    args = parser.parse_args(argv)

    if args.results_dir is None:
        results_dir = Path(
            runvault_path(EXPERIMENT, args.results_root, subcommand="run", standalone=True)
        )
    else:
        results_dir = resolve_results_dir(args.results_dir)
    if not results_dir.exists():
        print(f"エラー: ディレクトリが存在しません: {results_dir}", file=sys.stderr)
        return 1

    try:
        cfg, cfg_path, kind = _load_config(results_dir)
    except FileNotFoundError as exc:
        print(f"エラー: {exc}", file=sys.stderr)
        return 1

    meta = load_run_meta(results_dir, required=False) or {}
    # legacy の run は wide な metrics.csv (時間軸の列が `t`) を持つので，long 形式を
    # 前提にした run スコープ指標の読み出しを掛けない．
    scoped = run_scope_metrics(results_dir) if meta else {}
    legacy_llm = load_legacy_llm_meta(results_dir) if not meta else None

    if args.json:
        payload = {"source": str(cfg_path), "kind": kind, "config": cfg,
                   "llm": meta.get("llm"), "run_scope_metrics": scoped,
                   "legacy_llm_meta": legacy_llm}
        print(json.dumps(payload, indent=2, ensure_ascii=False))
        return 0

    if kind == "sweep":
        print(render_sweep_config(cfg, cfg_path))
    else:
        print(render_flat_config(cfg, cfg_path, kind))
    llm_block = render_llm(meta, scoped, legacy_llm)
    if llm_block is not None:
        print(llm_block)
    return 0


if __name__ == "__main__":
    sys.exit(main())
