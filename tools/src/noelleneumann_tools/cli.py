"""noelleneumann-tools — Noelle-Neumann (1974) The Spiral of Silence ツール統合 CLI．

Usage:
    noelleneumann-tools visualize [...]
    noelleneumann-tools visualize-sweep [...]
    noelleneumann-tools show-experiment-settings [...]
    noelleneumann-tools reproduce [...]

各サブコマンドに続く引数は，対応するモジュールの argparse がそのまま受け取る．
dispatcher の組み立ては共有ヘルパ `socsim_tools.cli.build_dispatcher` に委譲する．
"""

from __future__ import annotations

from socsim_tools.cli import build_dispatcher

main = build_dispatcher(
    prog="noelleneumann-tools",
    description="Noelle-Neumann (1974) The Spiral of Silence 可視化・分析ツール",
    subcommands={
        "visualize": (
            "単一実行結果 (螺旋軌跡・陣営別発言意欲) の可視化",
            "noelleneumann_tools.visualize:main",
        ),
        "visualize-sweep": (
            "スイープ結果 (η_m × β 境界マップ・α 相図) の可視化",
            "noelleneumann_tools.visualize_sweep:main",
        ),
        "show-experiment-settings": (
            "実行結果ディレクトリの設定 (config.json / sweep_config.json) の表示",
            "noelleneumann_tools.show_experiment_settings:main",
        ),
        "reproduce": (
            "論文 Table 1--5 アンカーと baseline 出力の照合レポート",
            "noelleneumann_tools.reproduce_paper:main",
        ),
    },
)


if __name__ == "__main__":
    main()
