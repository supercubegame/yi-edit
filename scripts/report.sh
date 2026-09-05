#!/usr/bin/env bash
# 把一份报告回写到我读得到的地方。两条通道：有 PR 写 PR 评论，没 PR 写提交评论。
# 两条都有去重（只在一条上实现 marker 去重，生产里看不出来：每次推送都是新 SHA）。
# 回写失败必须退非零：静默的回写失败与一切正常在面板上一模一样。
#
# 用法：report.sh <body.md> [marker 行号，默认 1]
# 每一种报告占一个 marker（主报告 / 快闸门原始日志 / 每个平台的原始日志），
# 否则它们会互相覆盖，而覆盖的表现是「那条日志本来就不存在」。
#
# **这个脚本跑在三个平台上，macOS 的 /bin/bash 是 3.2.57。**
# 所以：变量先初始化（配合 set -u）、不用 bash 4 的写法、不把命令替代塞进多行 printf。
# crates/meta/tests/gate.rs 里有一条扫描器守这一条。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

BODY_FILE=${1:?用法: report.sh <body.md> [marker 行号]}
MARKER_LINE=${2:-1}
# marker 只存一份（scripts/marker.txt）：两处必须逐字相同的字串不能拄两份。
MARKER=$(sed -n "${MARKER_LINE}p" scripts/marker.txt)
if [ -z "$MARKER" ]; then
  echo "REPORT FAIL: marker.txt 第 $MARKER_LINE 行是空的" >&2
  exit 1
fi

REPO=${GITHUB_REPOSITORY:?}
SHA=${GITHUB_SHA:?}
RUN_ID=${GITHUB_RUN_ID:-local}
SHORT=$(echo "$SHA" | cut -c1-7)

channel=""
existing=""
attempts=0
pr=""

BODY=$(printf '%s\n\n%s\n\n<sub>commit %s / run %s</sub>\n' \
  "$MARKER" "$(cat "$BODY_FILE")" "$SHORT" "$RUN_ID")

try() {
  n=0
  until "$@"; do
    n=$((n + 1))
    attempts=$((attempts + 1))
    if [ "$n" -ge 3 ]; then
      echo "REPORT: 重试 $n 次仍失败：$*" >&2
      return 1
    fi
    sleep 5
  done
  attempts=$((attempts + 1))
  return 0
}

if [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "${GITHUB_EVENT_PATH:-}" ] && command -v jq > /dev/null 2>&1; then
  pr=$(jq -r '.pull_request.number // empty' "$GITHUB_EVENT_PATH")
fi
if [ -z "$pr" ]; then
  pr=$(gh api "repos/$REPO/commits/$SHA/pulls" --jq '.[0].number // empty' 2> /dev/null || true)
fi

find_existing() {
  gh api "$1" --paginate \
    --jq "[.[] | select(.body != null and (.body | contains(\"$MARKER\")))] | .[0].id // empty" \
    2> /dev/null || true
}

if [ -n "$pr" ]; then
  channel="pr#$pr"
  existing=$(find_existing "repos/$REPO/issues/$pr/comments")
  if [ -n "$existing" ]; then
    try gh api -X PATCH "repos/$REPO/issues/comments/$existing" -f body="$BODY" > /dev/null
  else
    try gh api -X POST "repos/$REPO/issues/$pr/comments" -f body="$BODY" > /dev/null
  fi
else
  channel="commit $SHORT"
  existing=$(find_existing "repos/$REPO/commits/$SHA/comments")
  if [ -n "$existing" ]; then
    try gh api -X PATCH "repos/$REPO/comments/$existing" -f body="$BODY" > /dev/null
  else
    try gh api -X POST "repos/$REPO/commits/$SHA/comments" -f body="$BODY" > /dev/null
  fi
fi

# 尝试次数要进报告：否则「一次就过」与「第三次才过」长得一模一样，
# 上游在慢慢变差就看不见。
updated=no
if [ -n "$existing" ]; then
  updated=yes
fi
echo "marker_line=$MARKER_LINE channel=$channel attempts=$attempts updated_existing=$updated" \
  >> report-attempts.txt
echo "REPORT: marker 行 $MARKER_LINE 已写入 $channel（尝试 $attempts 次，复用已有评论：$updated）"
