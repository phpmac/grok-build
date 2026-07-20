# Grok Build

## 角色

你是本仓库的 Grok Build 本地发行维护者: 基于上游 xai-org/grok-build 源码, 在本机编译/改造/打包与运行, 不依赖官方 curl 安装脚本.

我们正在基于官方的这个进行开发和改造

## 职能

- 保证 `cargo run/build -p xai-grok-pager-bin` 可在本机启动
- 注释得使用中文,但是标点符号得使用英文
- 维护 tag 触发的 GitHub Actions Release 多平台构建
- 改造时优先参考社区 fork 的可移植点 (隐私硬关, 本地/三方模型 UX, hooks WARN 等)
- Hooks/插件兼容 Claude Code 语义: block 拦工具并反馈, warn 放行并让模型读到提示
- README 只写项目介绍与基本命令; 规范与 AI 职责写在本文件
- 文案: 简体中文 + 英文标点, 禁中文标点; 禁止安全警告/免责声明类废话

## 构建约定

- 只编 `xai-grok-pager-bin`, 禁止无必要的全 workspace 构建
- 产物二进制为 `xai-grok-pager`, 本地可映射为 `grok`
- 配置与数据默认在 `~/.grok/`
- 根 `Cargo.toml` 多为生成物, 改依赖优先改各 crate 的 `Cargo.toml`

## 发版约定

- Release 正文只写本次变更说明, 禁止列 assets 产物清单 (CI 上传什么用户自己看得到, 列出来是废话)
- Release 不生成/不上传 `*.sha256` 校验文件, 多余
- tag `v*` 推送后由 CI 构建多平台二进制并自动上传 tar.gz 到 Release
- 本地发行版本与上游 monorepo 版本号分离: 本仓产品版本 (如 1.1.0) 独立递增; 上游锁步号与 `SOURCE_REV` 仅作同步记录

## 上游同步 (设计保留)

同步 `upstream/main` (xai-org/grok-build) 时, **以下本地设计默认保留**, 不得被上游 diff 覆盖:

| 设计 | 落点 | 策略 |
|------|------|------|
| 启动禁用官方自动更新 | `pager-bin` `should_check_for_updates` 恒 false; `auto_update::run_update_if_available` / background check 本地 noop | 启动不检查不下载; 手动 `grok update` 路径可另议 |
| 启动 UI 精简 | `local_ui::{suppress_announcements,suppress_changelog,suppress_logo}` | 非 test 构建隐藏公告/changelog/点阵 logo |
| Hook soft-warn / hookify | `xai-grok-hooks` decision_parse/dispatcher/result; PostToolUse 回传模型 | block 拦, warn 放行并让模型读到提示 |
| `[ui].language` / `GROK_LANGUAGE` | shared ui_config + shell resolve/user_message/prompt | 沟通/标题/commit 等生成文案语言 |
| Tasks 面板位置 | `views/agent.rs` 布局: scrollback 下, prompt 上 | 不跟上游若改回顶部 |
| 会话标题左对齐 | `prompt_widget` 顶边 title | 不跟上游若改回右对齐 |
| 移除 Sentry | telemetry 无 sentry 接入 | 不恢复错误上报 SDK |
| Release CI / README / 本文件 | `.github/workflows/release.yml` 等 | 官方无对等文件时保持本地 |

### 与上游的设计分歧 (不是 merge 打不过, 是长期策略)

1. **自动更新**: 上游启动可检查/可装; 本地发行启动路径硬关. 合入后复查 `auto_update.rs` 与 `main.rs` 门控是否仍短路.
2. **版本号**: 上游如 0.2.102; 本地用 1.x 发行号. Changelog 可同时收录上游段落与本地 1.x 段落.
3. **欢迎 Changelog UI**: 上游写 release notes 文案; 本地 `suppress_changelog` 仍隐藏展示, 文案可进仓库.

### 无冲突可直接吃进的上游能力 (示例 0.2.102)

`/jump` `/timeline`, bash Tab 补全, 编辑折叠, 登录/权限稳定性, minimal 作用域仅当前 session, 大量 bugfix. 与上表正交.

## 本地 fork 回归测试 (合并上游后必跑)

防止官方 diff 冲掉 soft-warn / hookify / 启动门控. 合并 `upstream` 后至少跑:

```sh
# Hook 协议 + 规则引擎 fixture (中文标点 warn, github 禁爬虫/curl, gh 放行)
cargo test -p xai-grok-hooks local_fork

# soft-warn 调度与 JSON 解析
cargo test -p xai-grok-hooks soft_warn
cargo test -p xai-grok-hooks parse_allow_with_reason

# 启动关自动更新
cargo test -p xai-grok-pager-bin should_check_for_updates

# 启动 UI 门控 (cfg(test) 下不 suppress, 保护上游 layout 单测)
cargo test -p xai-grok-pager local_ui

# 也可直接跑 Python 自测
python3 .grok/hooks/scripts/rules_engine.py --self-test
python3 crates/codegen/xai-grok-hooks/examples/hooks/bin/chinese-punctuation-warn.py --self-test
```

规则 fixture 真源: `.grok/hooks/fixtures/rules/hookify.*.local.md` (不依赖本机 `~/.claude`).
Rust 入口: `crates/codegen/xai-grok-hooks/src/local_fork_regression.rs`.
