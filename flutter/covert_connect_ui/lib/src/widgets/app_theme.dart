import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/desktop/desktop_theme.dart';
import 'package:covert_connect/src/widgets/mobile/mobile_theme.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

typedef ThemeBuilder = Widget Function(BuildContext context, ThemeData theme, ThemeData darkTheme);

const kScaffoldBackgroundColor = Color(0xfff3f3f3);
const kScaffoldBackgroundColorDark = Color(0xff1C1C1C);

class AppTheme extends StatelessWidget {
  const AppTheme({super.key, required this.builder});

  final ThemeBuilder builder;

  @override
  Widget build(BuildContext context) {
    final themeData = ThemeData(
      brightness: Brightness.light,
      colorScheme: const ColorScheme.light(
        primary: Color(0xFF242C46),
        secondary: Color(0xFF0FBA81),
        tertiary: Color(0xFF4A93D7),
        onSecondary: Color(0xFFFBFBFB),
        onTertiary: Color(0xFFFBFBFB),
        onSurface: Color(0xFF1B1B1B),
        surface: Color(0xFFFBFBFB),
        outline: Color(0x15000000),
      ),
      extensions: const <ThemeExtension<dynamic>>[AppColors(scaffoldBackgroundSecondary: kScaffoldBackgroundColor)],
    );
    final themeDataDark = ThemeData(
      brightness: Brightness.dark,
      colorScheme: const ColorScheme.dark(
        primary: Color(0xFFE4E6EE), // 0xFFE4E6EE, 0xFFA7DBFC, 0xFF649DD4
        secondary: Color(0xB00FBA81),
        tertiary: Color(0xB0649DD4),
        onTertiary: Color(0xC0FFFFFF),
        onSecondary: Color(0xC0FFFFFF),
        onSurface: Color(0xD0FFFFFF),
        surface: Color(0xFF2C2B26),
        outline: Color(0x15FFFFFF),
      ),
      extensions: const <ThemeExtension<dynamic>>[AppColors(scaffoldBackgroundSecondary: kScaffoldBackgroundColorDark)],
    );
    final textTheme = Theme.of(context).textTheme.merge(themeData.textTheme);
    final textThemeDark = Theme.of(context).textTheme.merge(themeDataDark.textTheme);
    final theme = themeData.copyWith(
      textTheme: GoogleFonts.interTextTheme(textTheme),
      scaffoldBackgroundColor: kScaffoldBackgroundColor,
    );
    final darkTheme = themeDataDark.copyWith(
      textTheme: GoogleFonts.interTextTheme(textThemeDark),
      scaffoldBackgroundColor: kScaffoldBackgroundColorDark,
    );

    if (isDesktop) {
      return DesktopTheme(theme: theme, darkTheme: darkTheme, builder: builder);
    } else {
      return MobileTheme(theme: theme, darkTheme: darkTheme, builder: builder);
    }
  }
}

extension AppColorsExtension on BuildContext {
  AppColors get appColors => Theme.of(this).extension<AppColors>()!;
}

@immutable
class AppColors extends ThemeExtension<AppColors> {
  const AppColors({required this.scaffoldBackgroundSecondary});

  final Color? scaffoldBackgroundSecondary;

  @override
  AppColors copyWith({Color? scaffoldBackgroundSecondary}) {
    return AppColors(scaffoldBackgroundSecondary: scaffoldBackgroundSecondary ?? this.scaffoldBackgroundSecondary);
  }

  @override
  AppColors lerp(AppColors? other, double t) {
    if (other is! AppColors) {
      return this;
    }
    return AppColors(
      scaffoldBackgroundSecondary: Color.lerp(scaffoldBackgroundSecondary, other.scaffoldBackgroundSecondary, t),
    );
  }

  // Optional
  @override
  String toString() => 'AppColors(scaffoldBackgroundSecondary: $scaffoldBackgroundSecondary)';
}
