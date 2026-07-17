# Grok Build

终端 AI 编程 Agent, 本地编译运行.

## 使用

```sh
# 编译并启动
cargo run -p xai-grok-pager-bin

# 发布构建
cargo build -p xai-grok-pager-bin --release
./target/release/xai-grok-pager

# 可选
cp target/release/xai-grok-pager ~/.local/bin/grok
grok
```

Mac 上若 SIGKILL: `xattr -cr ~/.local/bin/grok`

## 配置

`~/.grok/config.toml` 常用项:

```toml
[ui]
language = "简体中文"    # 沟通/标题/commit 等生成文案语言; 也可设 GROK_LANGUAGE
```

启动时只显示版本, 不做官方自动更新检查.

## 发版

推送 `v*` 标签后, GitHub Actions 自动构建并上传 Release 产物.

```sh
git tag v0.1.0
git push origin v0.1.0
```

