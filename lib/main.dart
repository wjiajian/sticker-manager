import 'package:flutter/material.dart';

import 'platform/platform_bridge.dart';
import 'services/quick_picker_controller.dart';
import 'ui/app_theme.dart';
import 'ui/library_page.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await PlatformBridge.instance.initialize();
  await QuickPickerController.instance.initialize();
  runApp(const StickerManagerApp());
}

class StickerManagerApp extends StatelessWidget {
  const StickerManagerApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: '表情管家',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.themeData(),
      home: const LibraryPage(),
    );
  }
}
