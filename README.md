# Grok Build

终端 AI 编程 Agent, 本地编译运行.

## 使用

```sh
# 编译并启动
cargo run -p xai-grok-pager-bin

# 发布构建
cargo build -p xai-grok-pager-bin --release
./target/release/xai-grok-pager
```

可选:

```sh
cp target/release/xai-grok-pager ~/.local/bin/grok
grok
```

## 发版

推送 `v*` 标签后, GitHub Actions 自动构建并上传 Release 产物.

```sh
git tag v0.1.0
git push origin v0.1.0
```
