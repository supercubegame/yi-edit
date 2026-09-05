#!/usr/bin/env bash
# 红了就把原始输出贴成评论。
#
# 为什么需要它：CI 的运行日志没有任何可用工具读得到，产物也可能静默挂掉（已经挂过一次）。
# 那时候手里只剩一个比特，于是只能二分推送去猜哪一行错了，而每一次猜都是一次「读着判断」。
# 修法不需要新能力，只要把结论搬到已有的那条通道上。
#
# 这一步**不是断言**：它自己失败不能抢掉真正的失败，所以调用方把它放在最终判定之前。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

TITLE=${1:?用法: raw-log.sh <标题> <marker 行号> [日志文件...]}
MARKER_LINE=${2:?缺 marker 行号}
shift 2

out=raw-log.md
{
  printf '## %s\n\n' "$TITLE"
  if [[ $# -eq 0 ]]; then
    echo "（没指定任何日志文件）"
  fi
  for f in "$@"; do
    printf '### %s\n\n' "$f"
    if [[ -f $f ]]; then
      printf '```\n'
      tail -n 120 "$f"
      printf '\n```\n\n'
    else
      # 「文件不存在」与「文件是空的」要分得清：前者意味着那一步根本没跑到。
      printf '这个文件不存在（不是空，是没生成）。\n\n'
    fi
  done
} > "$out"

bash scripts/report.sh "$out" "$MARKER_LINE"
