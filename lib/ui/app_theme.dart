import 'package:flutter/material.dart';

/// Visual tokens and the Material theme for the library UI. The palette is
/// light gray with a teal accent reserved for primary actions, selection and
/// keyboard focus.
class AppTheme {
  const AppTheme._();

  static const Color background = Color(0xFFF7F8FA);
  static const Color sidebarBackground = Color(0xFFECEFF2);
  static const Color cardBackground = Color(0xFFFFFFFF);
  static const Color accent = Color(0xFF087F78);
  static const Color border = Color(0xFFE1E5E9);
  static const Color primaryText = Color(0xFF20242C);
  static const Color secondaryText = Color(0xFF737B87);

  static const double sidebarWidth = 216;
  static const double controlHeight = 38;
  static const double buttonRadius = 8;
  static const double cardRadius = 12;
  static const double contentPadding = 24;
  static const double gridSpacing = 16;

  static ThemeData themeData() {
    final colorScheme = ColorScheme.fromSeed(
      seedColor: accent,
      primary: accent,
      surface: cardBackground,
    );
    final base = ThemeData(useMaterial3: true, colorScheme: colorScheme);
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
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: primaryText,
          minimumSize: const Size(0, controlHeight),
          side: const BorderSide(color: border),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(buttonRadius),
          ),
        ),
      ),
    );
  }
}
