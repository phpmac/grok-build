# project-rules (真源)

Claude Code hookify 规则兼容. 本目录是真源, `~/.grok/hooks/` 通过软链接映射到这里.

规则目录:
- `~/.claude/hookify.*.local.md`
- `<项目>/.claude/hookify.*.local.md`

入口: `project-rules.json` -> `scripts/rules_engine.py`

## 软链接到全局

在仓库根目录执行:

```fish
mkdir -p ~/.grok/hooks && ln -sfn (pwd)/.grok/hooks/project-rules.json ~/.grok/hooks/project-rules.json && ln -sfn (pwd)/.grok/hooks/scripts ~/.grok/hooks/scripts && ln -sfn (pwd)/.grok/hooks/README.md ~/.grok/hooks/README.md
```

说明:
- Grok 扫描 `*.json` 时会跟随软链接
- 全局与项目若加载到同一 command, 会按内容去重, 只跑全局那份
- 改这里的文件后, 需新开 Grok 会话才生效
