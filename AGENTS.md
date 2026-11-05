# Project conventions

For every agent working in this repository.


## Real-window testing

- When a test requires operating the computer, prefer background mode: screenshot, click, and type only in the target app, without taking the foreground, raising its window, or moving the user's mouse. The user may be using this computer at the same time.
- Take over the full screen only for steps background mode cannot do (for example cross-app drag and drop, or keys that cannot be sent in the background). Keep it brief, hand control back as soon as you finish, and state in your report which steps took over the screen.
- Do not run real-window tests for drag-related features (drag-in import, drag-out to send, drag back, and so on). The user gives feedback on these through daily use; only check the related code for obvious errors.
- Always point test instances at a temporary directory with `STICKER_LIBRARY_DIR` (absolute path). Do not touch the real library at `~/Library/Application Support/com.wjiajian.sticker-manager` (0.1.3 and earlier used `com.shaojian.sticker-manager`; newer versions move it automatically on first launch).
- Do not change system settings (Low Data Mode, Reduce Motion, appearance, and so on). If a test needs one, ask the user to switch it themselves; if they do not, record the test as skipped.


## Development

- Frontend dependencies and scripts live in `frontend/`. From the repository root, run them with `pnpm --dir frontend <script>`, for example `pnpm --dir frontend tauri dev`.
- The website lives in `website/` and has its own dependencies, independent of `frontend/`. From the repository root, run it with `pnpm --dir website <script>`, for example `pnpm --dir website dev`.
- Run the checks in `docs/testing.md` before finishing a change.
- When adding a Tauri command, add it to the allowlists in `src-tauri/permissions/` for the windows that use it. Otherwise Tauri rejects the call at runtime, and neither the build nor the tests catch it.


## PR instructions

1. Title format: use conventional commit messages
2. Use English to write PR title and descriptions.


## Release versions

Use a short-lived `release/*` branch for each release. The release branch is the stabilization area for version bumps, changelog updates, release-blocking fixes, and final validation only. Do not add unrelated features or broad refactors to a release branch.

