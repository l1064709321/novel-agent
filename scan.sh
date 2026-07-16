#!/usr/bin/env bash
# scan.sh — 上传前安全扫描 (确保小说正文和凭证不会被推送到 GitHub)
# 用法: bash scan.sh
# 退出码 0 = 干净;非 0 = 发现问题
set -u
cd "$(dirname "$0")"

echo "==> [1/4] 确认 git 仓库..."
[ -d .git ] || { echo "✗ 不是 git 仓库,先 git init"; exit 1; }

echo "==> [2/4] 确认 secrets/ 已被 gitignore..."
if ! git check-ignore secrets/github.token >/dev/null 2>&1; then
  echo "✗ 危险: secrets/github.token 未被 gitignore 排除!"
  exit 2
fi
echo "  ✓ secrets/ 已排除"

echo "==> [3/4] 扫描暂存区,确保无小说正文和凭证..."
# 白名单: README.md 是本工作区说明,允许上传
ALLOW_RE='(^|/)README\.md$'
STAGED=$(git diff --cached --name-only 2>/dev/null)
if [ -n "$STAGED" ]; then
  echo "$STAGED" | grep -iE '\.(txt|docx|pdf|epub)$' && {
    echo "✗ 暂存区含小说正文文件,已拦截"
    exit 3
  } || true
  # .md 只允许 README.md,其他视为小说正文拦截
  echo "$STAGED" | grep -iE '\.md$' | grep -ivE "$ALLOW_RE" && {
    echo "✗ 暂存区含小说正文 .md(非 README),已拦截"
    exit 3
  } || true
  echo "$STAGED" | grep -iE '(token|secret|\.env|api[_-]?key)' | grep -ivE "$ALLOW_RE" && {
    echo "✗ 暂存区含凭证文件,已拦截"
    exit 4
  } || true
fi
echo "  ✓ 暂存区干净"

echo "==> [4/4] 扫描改动文件中的密钥模式..."
PATTERNS=(
  'sk-[a-zA-Z0-9_\-]{16,}'
  'sk-ant-[a-zA-Z0-9_\-]{16,}'
  'AIza[a-zA-Z0-9_\-]{16,}'
  'github_pat_[a-zA-Z0-9_]{20,}'
  'ghp_[a-zA-Z0-9]{20,}'
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
)
FILES=$(git status --short --untracked-files=all | awk '{print $2}')
HITS=0
for f in $FILES; do
  [ -f "$f" ] || continue
  git check-ignore -q "$f" 2>/dev/null && continue   # 跳过被忽略的文件
  file "$f" 2>/dev/null | grep -qi "text" || continue
  for pat in "${PATTERNS[@]}"; do
    if grep -qE "$pat" "$f" 2>/dev/null; then
      echo "✗ 命中敏感模式 [$pat] → $f"
      HITS=$((HITS+1))
    fi
  done
done
[ "$HITS" -gt 0 ] && { echo "✗ 发现 $HITS 处敏感信息"; exit 5; } || echo "  ✓ 无敏感信息"

echo ""
echo "✓ 扫描通过,可安全上传"
echo "  (secrets/ 和 novels/*.txt 已确认排除)"
