import 'package:flutter/material.dart';

/// Visual tokens and the Material theme for the library UI. The palette is
/// light gray with a teal accent reserved for primary actions, selection and
/// keyboard focus.
class AppTheme {
  const AppTheme._();

  static const Color background = Color(0xFFF6F8FA);
  static const Color sidebarBackground = Color(0xFFEAF0F4);
  static const Color cardBackground = Color(0xFFFFFFFF);
  static const Color searchBackground = Color(0xFFEEF2F6);
  static const Color selectionBackground = Color(0xFFD6ECEE);
  static const Color accent = Color(0xFF008B8B);
  static const Color hoverBorder = Color(0xFFB5DDE0);
  static const Color border = Color(0xFFDFE6ED);
  static const Color primaryText = Color(0xFF101828);
  static const Color secondaryText = Color(0xFF788698);

  static double thumbnailSize(TargetPlatform platform) =>
      platform == TargetPlatform.windows || platform == TargetPlatform.macOS
          ? 64
          : 56;

  static const double sidebarWidth = 240;
  static const double controlHeight = 48;
  static const double secondaryControlHeight = 40;
  static const double buttonRadius = 6;
  static const double cardRadius = 6;
  static const double dialogRadius = 8;
  static const double contentPadding = 20;
  static const double gridSpacing = 14;

  static ThemeData themeData() {
    final colorScheme = ColorScheme.fromSeed(
      seedColor: accent,
      primary: accent,
      surface: cardBackground,
    );
    final base = ThemeData(
      useMaterial3: true,
      colorScheme: colorScheme,
      fontFamilyFallback: const [
        'PingFang SC',
        'Microsoft YaHei',
        'Noto Sans CJK SC',
      ],
    );
    return base.copyWith(
      scaffoldBackgroundColor: background,
      dialogTheme: DialogThemeData(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(dialogRadius)),
      ),
      cardTheme: CardThemeData(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(cardRadius)),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(buttonRadius)),
        ),
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(buttonRadius)),
        ),
      ),
      iconButtonTheme: IconButtonThemeData(
        style: IconButton.styleFrom(
          shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(buttonRadius)),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
            shape: WidgetStatePropertyAll(RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(buttonRadius)))),
      ),
      chipTheme: base.chipTheme.copyWith(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius)),
      ),
      popupMenuTheme: PopupMenuThemeData(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius)),
      ),
      menuTheme: MenuThemeData(
          style: MenuStyle(
              shape: WidgetStatePropertyAll(RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(buttonRadius))))),
      bottomSheetTheme: BottomSheetThemeData(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(dialogRadius)),
      ),
      drawerTheme: DrawerThemeData(
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(dialogRadius)),
      ),
      textTheme: base.textTheme.apply(
        bodyColor: primaryText,
        displayColor: primaryText,
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: cardBackground,
        isDense: true,
        contentPadding:
            const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        hintStyle: const TextStyle(color: secondaryText, fontSize: 14),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(buttonRadius),
          borderSide: const BorderSide(color: border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(buttonRadius),
          borderSide: const BorderSide(color: border),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(buttonRadius),
          borderSide: const BorderSide(color: accent, width: 1.5),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          backgroundColor: accent,
          foregroundColor: Colors.white,
          minimumSize: const Size(0, controlHeight),
          padding: const EdgeInsets.symmetric(horizontal: 22),
          textStyle: base.textTheme.labelLarge!
              .copyWith(fontSize: 16, fontWeight: FontWeight.w500),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: primaryText,
          backgroundColor: cardBackground,
          minimumSize: const Size(0, controlHeight),
          padding: const EdgeInsets.symmetric(horizontal: 18),
          textStyle: base.textTheme.labelLarge!
              .copyWith(fontSize: 16, fontWeight: FontWeight.w400),
          side: const BorderSide(color: border),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius),
          ),
        ),
      ),
    );
  }
}
