# Repository Guidelines

## Project Structure & Module Organization

- `lib/main.dart` contains the Flutter UI; `lib/models.dart` defines domain models.
- `lib/services/` handles persistence, imports, ranking, preferences, and encrypted exports. Preserve the `StickerRepository` and `ImportSource` boundaries.
- `lib/platform/` contains platform bridges; `windows/runner/` and `android/app/src/main/` contain native integrations.
- `test/` holds automated tests; `tool/` contains verification, packaging, and installation scripts.
- Assets are declared in `pubspec.yaml`; the Windows icon is in `windows/runner/resources/`. Product specifications, architecture, and decisions are in `docs/`.

Windows, macOS, and Android are implemented.

## Build, Test, and Development Commands

Use Flutter 3.47.3 or a compatible SDK on PATH. Android requires JDK 17+, SDK Platform 36, and NDK 28.2.13676358. Run commands from the repository root:

- `flutter pub get`: restore dependencies.
- `flutter run -d windows`: launch on Windows.
- `flutter run -d <android-device>`: launch on an Android device.
- `dart format lib test`: format Dart code.
- `flutter analyze`: check analyzer and lint rules.
- `flutter test`: execute the automated suite.
- `node tool/verify_project.mjs`: verify required files and dependency declarations.
- On Windows, `powershell -ExecutionPolicy Bypass -File .\tool\build_release.ps1`: run tests and analysis, build Windows and Android releases, and create the Windows ZIP. Close running project instances first.

## Coding Style & Naming Conventions

Use two-space Dart indentation, `lower_snake_case.dart` filenames, `UpperCamelCase` types, and `lowerCamelCase` members. Follow `flutter_lints` and `analysis_options.yaml`, including single quotes and avoiding `print`. Keep native APIs behind platform bridges.

## Testing Guidelines

Use `flutter_test` and name files `*_test.dart`. Run focused tests with `flutter test test/database_test.dart`. Cover changed behavior, especially database migrations, import validation, and encrypted exports. Clean temporary resources with `addTearDown`. No numeric coverage threshold is configured. Verify tray, clipboard, hotkey, and overlay changes on the corresponding platform.

## Commit & Pull Request Guidelines

Use short imperative subjects, matching history such as `Make Windows package easier to launch`; Conventional Commit prefixes are not established. PRs should explain behavior changes, link relevant issues, report validation and affected platforms, and include screenshots for UI changes. Update `docs/` and `CHANGELOG.md` for behavior, schema, or format changes.

## Security & Configuration

Never modify QQ/WeChat source files or parse private databases. Delete only app-managed copies. Supply release credentials through the four `STICKER_RELEASE_*` variables documented in `README.md`; keep secrets and keystores untracked.
