#!/usr/bin/env bash
# 当前 PR 号。**两处必须一致的逻辑只能有一份真身。**
#
# 实测过分岜的后果：report.sh 从事件载荷里拿到 PR 号并把报告写进了 PR 评论，
# 而 attest.sh 只问了 commits/<sha>/pulls——PR 事件下 GITHUB_SHA 是合并提交，问不出 PR，
# 于是它去提交评论那条通道上找，找不到，判红。而它判得对：
# 结论确实没送到它看的那个地方。假的不是判据，是两份不同的解析。
#
# 跑在三个平台上，macOS 的 bash 是 3.2：变量先初始化、${var} 带花括号。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ROOT}"

pr=""

# 1. 事件载荷：pull_request 事件下这是唯一可靠的来源。
if [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "${GITHUB_EVENT_PATH:-}" ] && command -v jq > /dev/null 2>&1; then
  pr=$(jq -r '.pull_request.number // empty' "${GITHUB_EVENT_PATH}")
fi

# 2. 退回问 API：push 事件下 GITHUB_SHA 是真实提交，能问出关联的 PR。
if [ -z "${pr}" ] && [ -n "${GITHUB_REPOSITORY:-}" ] && [ -n "${GITHUB_SHA:-}" ]; then
  pr=$(gh api "repos/${GITHUB_REPOSITORY}/commits/${GITHUB_SHA}/pulls" \
    --jq '.[0].number // empty' 2> /dev/null || true)
fi

echo "${pr}"
