#!/usr/bin/env bash
# 把各个 job 的产物拼成一份能定位根因的报告。
# 判据很简单：只看那条评论，能不能定位到根因？不能就要补。
#
# **主报告不能把自己的证据寄存在另一条评论里。** 实测踩过：一次变异体审计里
# 两个平台的 marker 被弄成同一个，快闸门那条原始日志评论直接被盖掉，
# 而失败的测试名只在那条里 —— 于是一个本来能定位的失败又变成了一个比特。
set -Eeuo pipefail

out=${1:?用法: build-report.sh <out.md>}
gate_result=${GATE_RESULT:-unknown}
heavy_result=${HEAVY_RESULT:-unknown}
GATE_LOG_ART=artifacts/gate-evidence/gate.log

emoji() {
  case "$1" in
    success) echo "✅" ;;
    failure) echo "❌" ;;
    cancelled) echo "⚪" ;;
    skipped) echo "⚪" ;;
    *) echo "❓" ;;
  esac
}

{
  echo "## Yi Edit 闸门报告"
  echo
  echo "| 环节 | 结果 |"
  echo "| --- | --- |"
  echo "| 快闸门（core/fileio/meta） | $(emoji "${gate_result}") ${gate_result} |"
  echo "| 慢闸门（三平台构建 + 截图 + 基准） | $(emoji "${heavy_result}") ${heavy_result} |"
  echo
  echo "各平台的完整原始输出在单独的评论里（每个平台一条），但失败证据本条自带。"
  echo

  echo "### 快闸门"
  if [ -f artifacts/gate-evidence/gate-result.txt ]; then
    echo '```'
    cat artifacts/gate-evidence/gate-result.txt
    [ -f artifacts/gate-evidence/gate-metrics.txt ] && cat artifacts/gate-evidence/gate-metrics.txt
    echo '```'
  else
    echo "本次没拿到快闸门的产物（不是「通过了」，是**未确认**）。"
  fi
  echo

  # 失败摘要直接从产物里抽，不转手另一条评论。
  if [ "${gate_result}" != "success" ]; then
    echo "### 快闸门失败摘要（哪几条断言红了）"
    if [ -f "${GATE_LOG_ART}" ] && grep -q 'FAILURE SUMMARY' "${GATE_LOG_ART}"; then
      echo '```'
      sed -n '/FAILURE SUMMARY/,$p' "${GATE_LOG_ART}" | head -n 80
      echo '```'
    else
      echo "抽不到失败摘要（未确认）：产物里没有 gate.log 或它里面没有摘要节。"
    fi
    echo
  fi

  echo "### 大文件基准实测值"
  if [ -f artifacts/heavy-evidence/bench-result.txt ]; then
    echo '```'
    cat artifacts/heavy-evidence/bench-result.txt
    echo '```'
  else
    echo "本次没采到基准数据（未确认，不是通过）。"
  fi
  echo

  echo "### 截图检查"
  if [ -f artifacts/heavy-evidence/shotcheck.txt ]; then
    echo '```'
    cat artifacts/heavy-evidence/shotcheck.txt
    echo '```'
  else
    echo "本次没拿到截图检查输出（未确认）。"
  fi
  echo

  # 产物管道自己会静默挂（已经挂过一次：permissions 里没给 actions: read）。
  echo "<details><summary>产物目录实际内容（诊断用）</summary>"
  echo
  echo '```'
  if [ -d artifacts ]; then
    find artifacts -maxdepth 3 | head -n 60
  else
    echo "artifacts 目录不存在：下载产物那一步根本没拿到东西"
  fi
  echo '```'
  echo
  echo "</details>"
} > "${out}"

echo "已生成 ${out}（$(wc -l < "${out}") 行）"
