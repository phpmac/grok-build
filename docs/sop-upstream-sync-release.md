# SOP: 同步上游 → 合并处理 → 发版

触发语 (任一条即可执行本流程):

- 同步上游 / 合并上游 / 拉上游发版
- 合并线上最新 / 合并主分支上游
- 同步并发布新版本

真源仓库:

| remote | 地址 | 用途 |
|--------|------|------|
| `origin` | `phpmac/grok-build` | 本仓 main / tag / Release |
| `upstream` | `xai-org/grok-build` | 官方 monorepo 同步源 |

规范锚点: 根目录 `Claude.md` (本地设计保留表 + 禁止本机 cargo). 本 SOP 是可执行清单.

---

## 0. 前置

```sh
cd /path/to/grok-build
git status                    # 工作区应 clean; 有未提交改动先 commit 或 stash
git remote -v                 # 必须有 origin + upstream
# 若无 upstream:
# git remote add upstream git@github.com:xai-org/grok-build.git
```

禁止本机:

- `cargo build` / `cargo test` / `cargo run` (会堆 `target/`)
- 本机打 tar.gz 当正式产物

发现 `target/` 或 `dist/*.tar.gz` → 立刻删.

---

## 1. 拉最新并判断是否需要合

```sh
git fetch upstream
git fetch origin
git checkout main
git pull --ff-only origin main   # 若有本地未推提交, 先处理再 pull

echo "=== 本地 ==="
git log -1 --oneline HEAD
cat SOURCE_REV
rg -n '^version = ' crates/codegen/xai-grok-{pager,pager-bin,shell,version}/Cargo.toml
git tag -l 'v*' --sort=-v:refname | head -5

echo "=== 上游领先 ==="
git rev-list --count HEAD..upstream/main
git log --oneline HEAD..upstream/main | head -20
```

- **count = 0**: 无需合并; 若只发版本地未 tag 改动, 跳到 §5 (仅升版本/发 tag).
- **count ≥ 1**: 继续 §2.

读上游提交说明 (changelog 素材):

```sh
git log -1 --format=%B upstream/main | head -80
git show upstream/main:SOURCE_REV
git show upstream/main:crates/codegen/xai-grok-pager-bin/Cargo.toml | head -6
# 上游版本形如 0.2.x; 本地产品版本保持 1.x
```

---

## 2. 分支合并

命名: `sync/upstream-<上游锁步号>-<本地新版本>`  
例: 上游 0.2.114, 本地将升到 1.11.0 → `sync/upstream-0.2.114-1.11.0`

```sh
NEW_LOCAL=1.x.y          # 见 §4 版本规则
UPSTREAM_LOCK=0.2.xxx    # 从上游 Cargo.toml / commit msg 读
git checkout -b "sync/upstream-${UPSTREAM_LOCK}-${NEW_LOCAL}"
git merge upstream/main -m "WIP merge"
```

### 常见冲突 (按此解)

| 文件 | 策略 |
|------|------|
| `crates/codegen/xai-grok-{pager,pager-bin,shell,version}/Cargo.toml` 的 `version` | **本地产品号** `${NEW_LOCAL}`, 不要上游 `0.2.x` |
| `Cargo.lock` | **禁手拼** (v1.23.0 曾手拼连挂 3 次). 官方实践: lock 只能由 cargo 生成. 做法: 以上游原版 lock 为基底, 跑 `cargo metadata --format-version 1 > /dev/null` 触发最小变更补丁, 再用 `cargo metadata --locked` 复验; 本机禁 cargo 时用临时 workflow_dispatch 在 CI runner 上跑同一命令, 取 artifact 替换. `generate-lockfile` 是全量刷新 (会把依赖拉到最新, 可能撞 rust-toolchain 的 rustc 版本), 别用它 |
| `Cargo.lock` 中 telemetry 依赖段 | 上游若带回 sentry: lock 由 cargo 按 toml 重新 resolve 自动剔除, 不手删段 |
| `crates/codegen/xai-grok-shell/CHANGELOG.md` | 顶部写本地 `1.x` 段; 可保留上游 `0.2.x` 段在 `changelogs/` |
| `SOURCE_REV` | 吃上游 (merge 通常已自动) |
| soft-warn / auto_update / local_ui / 无 Sentry | **保留本地**, 见 §3 |

无冲突时仍要做 §3 门控复查 (上游可能静默加回 sentry / 打开自动更新).

---

## 3. 本地设计门控 (合并后必查)

对照 `Claude.md`"上游同步 (设计保留)"表, 至少 grep/读:

```sh
# 自动更新硬关
rg -n 'fn should_check_for_updates' -A6 crates/codegen/xai-grok-pager-bin/src/main.rs
# 应含: 恒 return false + never_checks 测试

rg -n 'pub async fn check_update_background|pub async fn run_update_if_available' \
  -A5 crates/codegen/xai-grok-update/src/auto_update.rs
# 应: check → BackgroundUpdateCheck::none(); run → Ok(false)

# 启动 UI
test -f crates/codegen/xai-grok-pager/src/local_ui.rs && \
  rg -n 'suppress_announcements|suppress_changelog|suppress_logo' \
  crates/codegen/xai-grok-pager/src/local_ui.rs

# soft-warn / hookify
test -f crates/codegen/xai-grok-hooks/src/decision_parse.rs
test -f crates/codegen/xai-grok-hooks/src/local_fork_regression.rs
# Observe 必须解析 stdout JSON (1.17 曾被上游冲掉, TUI/模型都没提示)
rg -n 'parse_observe_result' crates/codegen/xai-grok-hooks/src/runner/command.rs
rg -n 'post_tool_use_observe_stdout_json' crates/codegen/xai-grok-hooks/src/local_fork_regression.rs

# Hook warn/block 必须红字 (3fe74d81 吃上游曾冲掉, TUI 灰到看不见)
rg -n 'HookAnnotation' crates/codegen/xai-grok-pager/src/scrollback/blocks/session_event.rs | head
rg -n 'hook_annotation_renders_red_not_muted|accent_error' \
  crates/codegen/xai-grok-pager/src/scrollback/blocks/session_event.rs
# 应含: HookAnnotation 用 theme.accent_error; 单测 hook_annotation_renders_red_not_muted


# 无 Sentry
test ! -f crates/codegen/xai-grok-telemetry/src/sentry.rs && echo no_sentry_file
rg -n 'name = "sentry' Cargo.lock || echo no_sentry_lock
rg -n 'sentry' crates/codegen/xai-grok-telemetry/Cargo.toml || echo telemetry_clean

# language
rg -n 'GROK_LANGUAGE|resolve_language' \
  crates/codegen/xai-grok-shell/src/util/config/resolve/ui.rs | head

# 标题左对齐 (注释仍在即策略未丢)
rg -n 'Left-aligned|left-aligned' \
  crates/codegen/xai-grok-pager/src/views/prompt_widget/mod.rs | head

# Release CI 仍在
test -f .github/workflows/release.yml
```

任一门控被冲掉: **当场修回**, 再进 §4. 禁止带着门控失效去发版.

---

## 4. 版本号 + 文案

### 4.1 产品版本规则 (本地 1.x)

| 变更类型 | 递增 | 例 |
|----------|------|-----|
| 同步上游 bugfix / 小改 | 补丁 | 1.10.0 → 1.10.1 |
| 同步上游新功能 (向后兼容) | 次版本 | 1.10.0 → 1.11.0 |
| 破坏性 / 大重构 | 主版本 | 1.x → 2.0.0 |

上游 `0.2.x` **不**改写本地 1.x; 只记在 `SOURCE_REV` 与 changelog 文案.

### 4.2 改版本的文件 (四处 + lock)

```text
crates/codegen/xai-grok-pager-bin/Cargo.toml
crates/codegen/xai-grok-pager/Cargo.toml
crates/codegen/xai-grok-shell/Cargo.toml
crates/codegen/xai-grok-version/Cargo.toml
Cargo.lock   # 仅上述 4 个 package 的 version 行
```

### 4.3 CHANGELOG

文件: `crates/codegen/xai-grok-shell/CHANGELOG.md`

顶部插入本地段 (模板):

```markdown
# 1.X.Y - YYYY-MM-DD

## Features

- **同步上游 monorepo 0.2.Z** (`SOURCE_REV` 见 SOURCE_REV 文件): <要点 1-2 句>.
- 本地设计保留不变 (关自动更新 / 启动 UI / soft-warn / language / 去 Sentry 等).

## Notes

- 产品版本继续走本地 1.x; 上游锁步号 0.2.Z, SOURCE_REV 推进.
```

### 4.4 Claude.md

更新"版本号"示例中的上游号, 以及"无冲突可直接吃进的上游能力"示例段 (一句话列本批能力).

### 4.5 轻量自检 (无 target)

```sh
python3 .grok/hooks/scripts/rules_engine.py --self-test
python3 crates/codegen/xai-grok-hooks/examples/hooks/bin/chinese-punctuation-warn.py --self-test
# 可选: 有本地 hookify 改动时再跑相关 fixture
```

---

## 5. 合入路径: 优先 PR (推荐)

PR 更标准的原因 (本仓也采用):

- 大 diff (百级文件) 有 GitHub 审查面, 与 main 历史可追溯
- CI 可在合并前跑 (若以后加 branch protection)
- 回滚/对比用 PR URL, 比直接 push main 清晰
- 与 GitHub 常规协作一致; 本仓维护者仍可 squash/merge 自合

**仍允许** 紧急时 `ff-only` 直推 main (见 §5.2), 但默认走 PR.

### 5.1 PR 路径 (默认)

合并提交信息模板:

```text
合并: 同步上游 monorepo 0.2.Z 并升至 1.X.Y

- 接入 upstream <short-sha> (SOURCE_REV <8 位前缀>)
- 产品版本 A.B.C -> 1.X.Y; 保留 soft-warn/hookify 与启动门控/去 Sentry
- 上游: <要点>
- 冲突: <无 / 仅版本号与 CHANGELOG/Cargo.lock>
```

若 merge 时用了 `WIP`, 用 `git commit --amend` 换成正式信息 (分支未推远程前可 amend).

```sh
git add -A
# 若仍在 merge 中:
git commit   # 或 --amend 修正 WIP

git push -u origin "sync/upstream-${UPSTREAM_LOCK}-${NEW_LOCAL}"

gh pr create --repo phpmac/grok-build --base main --head "sync/upstream-${UPSTREAM_LOCK}-${NEW_LOCAL}"   --title "合并: 同步上游 monorepo 0.2.Z 并升至 1.X.Y"   --body "$(cat <<'EOF'
## 摘要

- 同步 upstream monorepo **0.2.Z** (`SOURCE_REV` ...)
- 产品版本 **1.X.Y**
- 本地设计门控已复查 (关更新 / soft-warn / 无 Sentry / language / local_ui)

## 测试

- [x] python rules_engine --self-test
- [x] chinese-punctuation-warn --self-test
- 完整 cargo 走 Release CI (本机不编)
EOF
)"

# 自合 PR (维护者本人)
gh pr merge --repo phpmac/grok-build --merge  # 或 --squash 若更偏好线性; 本仓历史常用 merge commit

git checkout main
git pull --ff-only origin main
git log -3 --oneline
```

### 5.2 直推 main (仅紧急)

```sh
git checkout main
git merge --ff-only "sync/upstream-${UPSTREAM_LOCK}-${NEW_LOCAL}"
git push origin main
```

---

## 6. tag + GitHub Release (在 main 合入之后)

`gh` 默认仓库可能指到 `xai-org/grok-build`, **必须** `--repo phpmac/grok-build`.

```sh
# 前提: main 已含同步提交 (PR 已 merge 或 §5.2 已 push)
NEW=1.X.Y
TAG="v${NEW}"
TITLE="同步上游 0.2.Z: <一句话中文主题>"   # 禁止 title 只写 v1.X.Y

git checkout main
git pull --ff-only origin main

git tag -a "${TAG}" -m "${TAG}: 同步上游 monorepo 0.2.Z"
git push origin "${TAG}"

gh release create "${TAG}" \
  --repo phpmac/grok-build \
  --target main \
  --title "${TITLE}" \
  --notes "$(cat <<'EOF'
## 变更

- 同步上游 monorepo **0.2.Z** (`SOURCE_REV` <短 hash>)
- 产品版本 **1.X.Y** (与上游锁步号分离; 上游为 0.2.Z)
- 本地设计保留: 关自动更新 / 启动 UI 精简 / soft-warn+hookify / language / Tasks 布局 / 标题左对齐 / 无 Sentry

## 上游亮点 (本批)

- <bullet 列表, 只写本版>

## 安装

从本 Release 的 Assets 下载对应平台 `grok-*.tar.gz`, 解压后将 `grok` 放到 PATH 即可.
EOF
)"
```

规则:

- tag 与 Release **都要有**; 禁止只打 tag
- notes **禁止**列 assets 清单 / 禁止 `*.sha256`
- title 中文概括主题, 版本号由 tag 承载
- **打 tag 前必须过完整编译验证** (v1.23.0 / v1.20.2 曾拿 Release 当第一次编译连挂): 先 push `main`, 等 `.github/workflows/precheck.yml` (`cargo check --locked -p xai-grok-pager-bin`) 全绿, 才允许 push tag. 本机禁 cargo; 不要用 tag Release 试编译

---

## 7. 验证

```sh
gh api repos/phpmac/grok-build/releases/tags/v1.X.Y \
  --jq '{name,tag_name,html_url,assets:[.assets[].name]}'
gh run list --repo phpmac/grok-build --limit 3
```

期望:

1. Release URL 可访问
2. workflow `Release` 被 tag 触发 (约 25–35 min)
3. 完成后 assets 含三平台:
   - `grok-aarch64-apple-darwin.tar.gz`
   - `grok-x86_64-unknown-linux-gnu.tar.gz`
   - `grok-aarch64-unknown-linux-gnu.tar.gz`

监控示例 (可选):

```sh
RUN_ID=$(gh run list --repo phpmac/grok-build --limit 1 --json databaseId -q '.[0].databaseId')
gh run watch "$RUN_ID" --repo phpmac/grok-build --exit-status
```

用户明确要求通知时, 成功后 `notify` 发 Discord/Telegram (禁止擅自发).

---

## 8. 收尾

```sh
rm -rf target
rm -f dist/*.tar.gz
git status   # clean
```

---

## 附录 A: 一次到位检查清单

- [ ] `git fetch upstream && fetch origin`, main 与 origin 对齐
- [ ] `HEAD..upstream/main` 有提交才 merge
- [ ] 分支名 `sync/upstream-0.2.Z-1.X.Y`
- [ ] 冲突: 产品 version=1.X.Y; 设计门控通过
- [ ] SOURCE_REV = 上游 tip 的 monorepo rev
- [ ] CHANGELOG + Claude.md 已更新
- [ ] python 自检通过
- [ ] merge commit 中文说明完整
- [ ] push sync 分支 + `gh pr create` + `gh pr merge`
- [ ] main 已含合入; annotated tag + `gh release create --repo phpmac/grok-build`
- [ ] CI 三平台 / assets 齐
- [ ] 无残留 `target/`

## 附录 B: 故障速查

| 现象 | 处理 |
|------|------|
| `gh release create` 报 tag 不在 xai-org | 加 `--repo phpmac/grok-build` |
| CI `cargo build --locked` 失败 | 查 Cargo.lock 本地 version / 是否误锁 sentry; **移除 lock 依赖逐行确认**: 冲突块内相邻行可能含合法依赖 (如 rustls 是 dev-dep), 禁整块删 |
| 合并后启动又检查更新 | 复查 `should_check_for_updates` 与 auto_update noop |
| 合并后有 sentry.rs | 按本地策略删模块 + 去 Cargo 依赖 + 清 lock |
| 本机磁盘爆 | `rm -rf target`; 禁止再 cargo |

## 附录 C: 与通用 release skill 的关系

全局 `release-workflow` 管"tag + Release + 版本递增"通用规则.  
**本仓以本 SOP 为准**: 必须先合 `upstream/main`, 保留本地设计, 产品 1.x 与上游 0.2.x 分离, 产物只走 GitHub Actions.
