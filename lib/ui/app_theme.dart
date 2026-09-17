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

  static const double sidebarWidth = 266;
  static const double controlHeight = 54;
  static const double secondaryControlHeight = 46;
  static const double buttonRadius = 12;
  static const double cardRadius = 14;
  static const double contentPadding = 24;
  static const double gridSpacing = 16;

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
              .copyWith(fontSize: 18, fontWeight: FontWeight.w500),
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
              .copyWith(fontSize: 18, fontWeight: FontWeight.w400),
          side: const BorderSide(color: border),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius),
          ),
        ),
      ),
    );
  }
}
