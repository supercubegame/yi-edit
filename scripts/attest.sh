#!/usr/bin/env bash
# 核对「结论真的送达了」。这不是一条被合成进报告的断言，而是跑在报告之后的独立核对。
#
# 四个关键设计：
# 1. 两条通道都查（只查一条，另一条坏掉时它会安静通过）。
# 2. 铉在本次运行上（同时含本次短 SHA 与 run id）—— 「存在一条带 marker 的评论」
#    是幂等写入的经典假绿。
# 3. 轮询，不是睡一觉。
# 4. 自证：同一套查找器在一个必然不存在的 marker 上必须返回 0 条。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

MARKER=$(head -n 1 scripts/marker.txt)
REPO=${GITHUB_REPOSITORY:?}
SHA=${GITHUB_SHA:?}
RUN_ID=${GITHUB_RUN_ID:?}
SHORT=${SHA:0:7}

count_matching() {
  local marker=$1
  local total=0 n
  local pr
  pr=$(gh api "repos/$REPO/commits/$SHA/pulls" --jq '.[0].number // empty' 2>/dev/null || true)
  if [[ -n $pr ]]; then
    n=$(gh api "repos/$REPO/issues/$pr/comments" --paginate \
      --jq "[.[] | select(.body != null and (.body | contains(\"$marker\")) and (.body | contains(\"$SHORT\")) and (.body | contains(\"$RUN_ID\")))] | length" 2>/dev/null || echo 0)
    total=$((total + ${n:-0}))
  fi
  n=$(gh api "repos/$REPO/commits/$SHA/comments" --paginate \
    --jq "[.[] | select(.body != null and (.body | contains(\"$marker\")) and (.body | contains(\"$SHORT\")) and (.body | contains(\"$RUN_ID\")))] | length" 2>/dev/null || echo 0)
  total=$((total + ${n:-0}))
  echo "$total"
}

# 自证先跑：查找器对一个不存在的 marker 必须返回 0。
bogus=$(count_matching "<!-- yi-edit-marker-that-must-not-exist-$RUN_ID -->")
if [[ ${bogus:-0} -ne 0 ]]; then
  echo "ATTEST FAIL: 查找器在一个不存在的 marker 上也找到了 $bogus 条，它是装饰" >&2
  exit 1
fi
echo "ATTEST: 自证通过（不存在的 marker 返回 0 条）"

for i in $(seq 1 12); do
  found=$(count_matching "$MARKER")
  if [[ ${found:-0} -ge 1 ]]; then
    echo "ATTEST OK: 第 $i 次轮询找到 $found 条带 marker + $SHORT + run $RUN_ID 的评论"
    exit 0
  fi
  echo "ATTEST: 第 $i 次轮询还没看到，10s 后重试"
  sleep 10
done

echo "ATTEST FAIL: 本次运行的结论一条也没送达（两条通道都查过）。送不出结论的闸门等于没跑。" >&2
exit 1
