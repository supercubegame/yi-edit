#!/usr/bin/env bash
# 核对「结论真的送达了」。这不是一条被合成进报告的断言，而是跑在报告之后的独立核对。
#
# 四个关键设计：
# 1. 两条通道都查（只查一条，另一条坏掉时它会安静通过）。
# 2. 铉在本次运行上（同时含本次短 SHA 与 run id）—— 「存在一条带 marker 的评论」
#    是幂等写入的经典假绿。
# 3. 轮询，不是睡一觉。
# 4. 自证：同一套查找器在一个必然不存在的 marker 上必须返回 0 条。
#
# PR 号必须与 report.sh 用同一份解析（scripts/pr-number.sh）：实测过分岜的后果——
# 它把一个完全正常的回写判成了「没送达」。
# 与 report.sh 同理：它也可能跑在 bash 3.2 上，不用 bash 4 的写法。
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ROOT}"

MARKER=$(sed -n '1p' scripts/marker.txt)
REPO=${GITHUB_REPOSITORY:?}
SHA=${GITHUB_SHA:?}
RUN_ID=${GITHUB_RUN_ID:?}
SHORT=$(echo "${SHA}" | cut -c1-7)
PR=$(bash scripts/pr-number.sh)

echo "ATTEST: channels = pr#${PR:-none} + commit ${SHORT}"

count_matching() {
  marker=$1
  total=0
  n=0
  if [ -n "${PR}" ]; then
    n=$(gh api "repos/${REPO}/issues/${PR}/comments" --paginate \
      --jq "[.[] | select(.body != null and (.body | contains(\"${marker}\")) and (.body | contains(\"${SHORT}\")) and (.body | contains(\"${RUN_ID}\")))] | length" 2> /dev/null || echo 0)
    total=$((total + n))
  fi
  n=$(gh api "repos/${REPO}/commits/${SHA}/comments" --paginate \
    --jq "[.[] | select(.body != null and (.body | contains(\"${marker}\")) and (.body | contains(\"${SHORT}\")) and (.body | contains(\"${RUN_ID}\")))] | length" 2> /dev/null || echo 0)
  total=$((total + n))
  echo "${total}"
}

# 自证先跑：查找器对一个不存在的 marker 必须返回 0。
bogus=$(count_matching "<!-- yi-edit-must-not-exist-${RUN_ID} -->")
if [ "${bogus}" -ne 0 ]; then
  echo "ATTEST FAIL: bogus marker matched ${bogus} comments, the finder is decoration" >&2
  exit 1
fi
echo "ATTEST: selftest passed (bogus marker returns 0)"

i=1
while [ "${i}" -le 12 ]; do
  found=$(count_matching "${MARKER}")
  if [ "${found}" -ge 1 ]; then
    echo "ATTEST OK: poll ${i} found ${found} comment(s) with marker + ${SHORT} + run ${RUN_ID}"
    exit 0
  fi
  echo "ATTEST: poll ${i} found nothing yet, retrying in 10s"
  sleep 10
  i=$((i + 1))
done

echo "ATTEST FAIL: this run delivered no conclusion on either channel. A gate that cannot send its conclusion did not run." >&2
exit 1
