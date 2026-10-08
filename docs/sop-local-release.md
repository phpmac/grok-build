# 入职记录: 日常改动本地发版流程

适用: 日常代码改动后的交付与发布 (上游同步另有 `sop-upstream-sync-release.md`).
历史背景: CI 曾两次建删 (`79bf31e4` 删, `beba1e56` 恢复, `f9f2e417` 删), 现行为**发版回归本机构建**, 无 GitHub Actions.

## 固定流程 (每次都按这个走)

1. **改代码**: 在 main 直接做, 不新建分支.
2. **升版本**: 产品版本按 1.x 递增 (patch/minor 看变更性质), 四处同步改:
   - `crates/codegen/xai-grok-pager/Cargo.toml`
   - `crates/codegen/xai-grok-shell/Cargo.toml`
   - `crates/codegen/xai-grok-pager-bin/Cargo.toml`
   - `crates/codegen/xai-grok-version/Cargo.toml`
   `Cargo.lock` 禁手拼, 由构建自动更新.
3. **写 CHANGELOG**: `crates/codegen/xai-grok-shell/CHANGELOG.md`, Keep a Changelog 格式, `# X.Y.Z - 日期` 置顶, 记变更 + Notes (上游是否变化 / 版本递增).
4. **本机构建** (此时允许 cargo, 用完即清):
   ```sh
   cargo build --release -p xai-grok-pager-bin
   mkdir -p dist/grok-aarch64-apple-darwin
   cp target/release/xai-grok-pager dist/grok-aarch64-apple-darwin/grok
   ```
   产物二进制名 `xai-grok-pager`, 本地映射为 `grok`, 放 `dist/grok-aarch64-apple-darwin/` 交给用户.
5. **提交推送**: commit 中文说明 (变更要点 + 版本递增), push `origin main`. 用户未点名不打 tag, 不发 GitHub Release.
6. **清理 (强制)**: 编完立刻删, 磁盘曾经被 100G+ target 撑爆:
   ```sh
   rm -rf target
   rm -f dist/*.tar.gz   # dist 里的可执行文件留下
   ```

## 约束

- `CLAUDE.md` 只能人工编写, AI 禁改 (hookify `block-claude-md-human-only`); 新增本地设计需要登记时, 给出行文本请维护者手贴设计表.
- 新增本地设计行为要在 merge.rs / pager 侧留单测, 防上游同步冲掉.
- 构建失败就停: 不推送, 不交付.
