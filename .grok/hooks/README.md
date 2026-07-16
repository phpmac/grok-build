# Claude Code hookify 规则兼容

Grok 中转: 复用 Claude Code 的 `hookify.*.local.md` 规则, 输出 allow/deny.

## 规则目录

- `~/.claude/hookify.*.local.md`
- `<项目>/.claude/hookify.*.local.md`

## 安装规则

```fish
rm -rf ~/.claude/hook*
cd /path/to/hookify/examples
ln -sf (pwd)/hookify.*.local.md ~/.claude/
```

## 入口

`project-rules.json` -> `scripts/rules_engine.py` (pre / post / stop)
