import 'package:covert_connect/src/widgets/app_theme.dart';
import 'package:flutter/material.dart';

class MobileTheme extends StatelessWidget {
  const MobileTheme({super.key, required this.theme, required this.darkTheme, required this.builder});

  final ThemeData theme;
  final ThemeData darkTheme;
  final ThemeBuilder builder;

  @override
  Widget build(BuildContext context) {
    return builder(context, theme, darkTheme);
  }
}
