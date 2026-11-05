<!-- Title: use a conventional commit message, for example `fix(panel): keep paging after paste`. Write the title and description in English. -->
<!-- 标题：使用约定式提交格式，例如 `fix(panel): keep paging after paste`。标题和描述用英文书写。 -->

## Motivation / 动机

<!-- What problem does this solve? Link the related issue, for example `Fixes #123`. -->
<!-- 这项改动解决了什么问题？请关联相应的 issue，例如 `Fixes #123`。 -->

## Changes / 改动

<!-- Summarize what changed. Mention platform-specific behavior. -->
<!-- 概括改了什么，并说明各平台行为上的差异。 -->

- [x] This is not a breaking change. / 这不是破坏性变更。
<!-- Uncheck the box if it is, and describe the impact on existing libraries, sync data, or settings. -->
<!-- 如果是破坏性变更，请取消勾选，并说明对现有仓库、同步数据或设置的影响。 -->

## Verification / 验证

<!-- List what you ran and tested, with screenshots or logs for UI changes. State what was not verified, such as Windows or drag features. -->
<!-- 列出运行和测试过的内容，界面改动请附截图或日志，并写明没有验证的部分，例如 Windows 或拖拽功能。 -->

## Checklist / 检查清单

- [ ] `cargo test --workspace`, `pnpm --dir frontend typecheck`, and `pnpm --dir frontend build` pass.
  / 以上测试、类型检查和构建都已通过。
- [ ] New Tauri commands are added to the allowlists in `src-tauri/permissions/` for the windows that use them.
  / 新增的 Tauri 命令已加进 `src-tauri/permissions/` 里对应窗口的白名单。
- [ ] User-visible changes are recorded in `CHANGELOG.md`.
  / 用户能感知的改动已写进 `CHANGELOG.md`。
- [ ] Affected documentation is updated, or no update is needed.
  / 受影响的文档已更新，或无需更新。
