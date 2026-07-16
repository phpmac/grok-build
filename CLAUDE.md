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

