#!/usr/bin/env bash
# 把各个 job 的产物拼成一份能定位根因的报告。
# 判据很简单：只看那条评论，能不能定位到根因？不能就要补。
set -Eeuo pipefail

out=${1:?用法: build-report.sh <out.md>}
gate_result=${GATE_RESULT:-unknown}
heavy_result=${HEAVY_RESULT:-unknown}

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
  echo "| 快闸门（core/fileio/meta） | $(emoji "$gate_result") $gate_result |"
  echo "| 慢闸门（三平台构建 + 截图 + 基准） | $(emoji "$heavy_result") $heavy_result |"
  echo

  if [[ -f artifacts/gate-evidence/gate-result.txt ]]; then
    echo "### 快闸门"
    echo '```'
    cat artifacts/gate-evidence/gate-result.txt
    [[ -f artifacts/gate-evidence/gate-metrics.txt ]] && cat artifacts/gate-evidence/gate-metrics.txt
    echo '```'
  else
    echo "### 快闸门"
    echo "本次没拿到快闸门的产物（不是「通过了」，是**未确认**）。"
  fi
  echo

  if [[ -f artifacts/gate-evidence/gate.log ]] && [[ "$gate_result" != "success" ]]; then
    echo "### 快闸门失败输出（尾 80 行）"
    echo '```'
    tail -n 80 artifacts/gate-evidence/gate.log
    echo '```'
    echo
  fi

  echo "### 大文件基准实测值"
  if [[ -f artifacts/heavy-evidence/bench-result.txt ]]; then
    echo '```'
    cat artifacts/heavy-evidence/bench-result.txt
    echo '```'
    echo "首轮不设性能阈值：没有实测值就写下限只会制造假红。下一轮拿这些数字留三倍余量再设。"
  else
    echo "本次没采到基准数据（未确认，不是通过）。"
  fi
  echo

  echo "### 截图检查"
  if [[ -f artifacts/heavy-evidence/shotcheck.txt ]]; then
    echo '```'
    cat artifacts/heavy-evidence/shotcheck.txt
    echo '```'
  else
    echo "本次没拿到截图检查输出（未确认）。"
  fi
  if [[ -f artifacts/heavy-evidence/app-run.txt ]]; then
    echo
    echo "<details><summary>截图运行日志（尾 40 行，包含字体选择）</summary>"
    echo
    echo '```'
    tail -n 40 artifacts/heavy-evidence/app-run.txt
    echo '```'
    echo
    echo "</details>"
  fi
  echo

  echo "### 三平台构建"
  if [[ -f artifacts/build-matrix/results.txt ]]; then
    echo '```'
    cat artifacts/build-matrix/results.txt
    echo '```'
  else
    echo "构建结果看上面的慢闸门行（每个平台的产物在 Actions 页面的 artifacts 里）。"
  fi
} > "$out"

echo "已生成 $out（$(wc -l < "$out") 行）"
