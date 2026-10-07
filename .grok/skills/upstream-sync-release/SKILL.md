---
name: upstream-sync-release
description: 当用户要求 "同步上游", "合并上游", "拉上游发版", "合并线上最新", "合并主分支上游", "同步并发布", "上游合并发版", 或需要按本仓流程把 xai-org/grok-build 合进 main 并打 tag 发 GitHub Release 时使用. 完整步骤见 docs/sop-upstream-sync-release.md.
---

# 上游同步 + 发版 (本仓)

## 必读

发版步骤按 `.grok/rules/merge-local-release.md`. `docs/sop-upstream-sync-release.md` 只用来解冲突和查设计保留, 不要按它走 PR.

不要只按全局 `release-workflow` 打 tag. 合并后的构建和发版以 `.grok/rules/merge-local-release.md` 为准: 本机零构建, push main 等 Precheck CI 绿, tag 触发 `release.yml` 出包.

1. 在当前分支 merge `upstream/main`, 不新建分支
2. 保留 `Claude.md` 本地设计表 (关更新 / local_ui / soft-warn / language / 无 Sentry / 明文提示词). 不要改 `Claude.md`
3. 产品版本 1.x 独立递增; 上游锁步号只记 SOURCE_REV
4. commit 并 `git push origin HEAD`, 等 Precheck CI 全绿
5. 打 tag 触发 `release.yml` 由 GitHub Actions 构建 tar.gz 并上传; `gh release create --repo phpmac/grok-build` 先写好中文说明

## 最短执行序

```
fetch → merge upstream/main (当前分支)
→ 门控复查 → 升 1.x + CHANGELOG
→ python 自检 → commit → push main → Precheck CI 绿
→ tag v* → gh release create 写说明 → release.yml 产包上传
```

## 触发后行为

用户说"同步上游 / 合并发版"等 → 直接按 `.grok/rules/merge-local-release.md` 做完, 不必再问是否发版.
仅当工作区有未说明的脏改动时, 先问清 commit 还是 stash.
