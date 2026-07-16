# 小说工作区 (Novels Workspace)

此目录独立于 novel-agent 代码仓库,用于:
1. **存放 GitHub 凭证** — `secrets/github.token`(本地保留,我每次会话可扫描读取)
2. **存放小说成果** — `novels/` 子目录(agent 跑完的正文,本地保留)
3. **上传脚本** — `upload.sh`(扫描敏感信息 + 推送 GitHub)

## 目录结构

```
/workspace/novels/
├── secrets/              # 本地敏感信息 (不上传)
│   ├── github.token      # GitHub PAT (我读取用,被 gitignore)
│   └── .gitignore
├── novels/               # 小说成果 (不上传)
│   ├── .gitignore
│   └── README.md
├── upload.sh             # 上传脚本 (上传)
├── scan.sh               # 扫描脚本 (上传前安全检查)
└── .gitignore
```

## 使用方式

### AI 助手自动推送

每次会话我都能扫到 `secrets/github.token`,推送时直接读,无需用户重复提供 token。

### 手动上传

```bash
# 扫描敏感信息 (确保 novels/ 正文和 secrets/ 不会上传)
bash scan.sh

# 推送脚本到 GitHub
bash upload.sh
```

## 安全保证

- `secrets/` 整个目录被 .gitignore 排除 → token 永不上传
- `novels/*.txt` 等小说正文被排除 → 小说内容只留本地
- `scan.sh` 上传前再扫一遍 → 双保险
