#!/usr/bin/env bash
# upload.sh — 扫描 + 推送到 GitHub
# 自动从 secrets/github.token 读取凭证,无需手动输入
# 用法: bash upload.sh [commit-message]
set -u
cd "$(dirname "$0")"

MSG="${1:-update novels workspace}"

# 1. 读取凭证
TOKEN_FILE="secrets/github.token"
[ -f "$TOKEN_FILE" ] || { echo "✗ 凭证文件不存在: $TOKEN_FILE"; exit 1; }
source "$TOKEN_FILE"
[ -n "$GITHUB_TOKEN" ] || { echo "✗ GITHUB_TOKEN 为空"; exit 1; }

echo "==> 仓库: ${GITHUB_USER}/${GITHUB_REPO} 分支: ${GITHUB_BRANCH}"

# 2. 扫描
bash scan.sh || { echo "✗ 扫描未通过,已中止"; exit 2; }

# 3. 提交
git add -A
git commit -q -m "$MSG" 2>/dev/null || { echo "⚠ 无改动需要提交"; }

# 4. 推送 (token 仅用于本次 push,不写入 git config)
REMOTE="https://${GITHUB_USER}:${GITHUB_TOKEN}@github.com/${GITHUB_USER}/${GITHUB_REPO}.git"
git push "${REMOTE}" "${GITHUB_BRANCH}" 2>&1 | tail -5

# 5. 清理: 确保 remote URL 不含 token
git remote set-url origin "https://github.com/${GITHUB_USER}/${GITHUB_REPO}.git" 2>/dev/null
echo "==> token 已从 remote URL 清理"
echo "✓ 推送完成: https://github.com/${GITHUB_USER}/${GITHUB_REPO}"
