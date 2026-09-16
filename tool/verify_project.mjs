import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname.replace(/^\/(\w):/, '$1:');
const required = [
  'pubspec.yaml',
  'lib/main.dart',
  'lib/models.dart',
  'lib/services/database.dart',
  'lib/services/repository.dart',
  'lib/services/import_source.dart',
  'lib/services/media_store.dart',
  'lib/services/export_service.dart',
  'lib/platform/platform_bridge.dart',
  'android/app/src/main/AndroidManifest.xml',
  'android/app/src/main/kotlin/com/example/sticker_manager/MainActivity.kt',
  'macos/Runner/MainFlutterWindow.swift',
  'macos/Runner/AppDelegate.swift',
  'macos/Runner.xcodeproj/project.pbxproj',
  'macos/Runner/Release.entitlements',
  'tool/generate_macos_icons.sh',
  '.github/workflows/ci.yml',
  'tool/build_macos.sh',
  'tool/install_windows.ps1',
  'tool/uninstall_windows.ps1',
];
const missing = required.filter((file) => !existsSync(join(root, file)));
if (missing.length) {
  console.error(`Missing required project files: ${missing.join(', ')}`);
  process.exit(1);
}
const pubspec = readFileSync(join(root, 'pubspec.yaml'), 'utf8');
for (const dependency of ['sqflite', 'crypto', 'cryptography', 'file_picker', 'hotkey_manager']) {
  if (!new RegExp(`^  ${dependency}:`, 'm').test(pubspec)) {
    console.error(`Missing dependency: ${dependency}`);
    process.exit(1);
  }
}
const mainDart = readFileSync(join(root, 'lib/main.dart'), 'utf8');
if (!mainDart.includes('FilePicker.pickFiles(')) {
  console.error('File import must allow selecting multiple stickers');
  process.exit(1);
}
console.log(`Project structure OK (${required.length} required files, ${pubspec.split('\n').length} pubspec lines)`);
