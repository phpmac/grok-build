# TODO

- [x] 需要新增语言功能 这样在自动生成标题的时候 就使用那个语言来自动生成 包括commit之类的
  - 配置: `~/.grok/config.toml` 的 `[ui] language = "简体中文"` (或环境变量 `GROK_LANGUAGE`)
  - 生效点: system prompt / user_info / session title 生成; commit/PR 文案走同一语言指令

