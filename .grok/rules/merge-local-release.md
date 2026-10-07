合并或同步上游: 当前分支做完, 不新建分支, 不再问.

- 保留 Claude.md 本地设计
- 产品版本按本地 1.x 往上升
- commit 后推 main, Precheck CI 全绿才打 tag
- `gh release create --repo phpmac/grok-build` 写中文说明; tar.gz 由 `release.yml` 在 GitHub Actions 构建上传
- 本机禁 cargo 打包, 不留 target/ 与 dist
- CI 构建失败就停, 修好再发
- 本条覆盖 "未点名禁止改版本, 打包, 推远程"
- 2026-10-07 起构建回归 GitHub CI, 本机打包作废
