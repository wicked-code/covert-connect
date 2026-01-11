import 'package:covert_connect/src/widgets/app_theme.dart';
import 'package:flutter/material.dart';

class DesktopTheme extends StatefulWidget {
  const DesktopTheme({super.key, required this.theme, required this.darkTheme, required this.builder});

  final ThemeData theme;
  final ThemeData darkTheme;
  final ThemeBuilder builder;

  @override
  State<DesktopTheme> createState() => DesktopThemeState();
}

class DesktopThemeState extends State<DesktopTheme> {
  @override
  Widget build(BuildContext context) {
    const progressIndicatorTheme = ProgressIndicatorThemeData(
      constraints: BoxConstraints(minWidth: 22.0, minHeight: 22.0),
      strokeWidth: 3,
    );

    return widget.builder(
      context,
      widget.theme.copyWith(scaffoldBackgroundColor: Colors.transparent, progressIndicatorTheme: progressIndicatorTheme),
      widget.darkTheme.copyWith(scaffoldBackgroundColor: Colors.transparent, progressIndicatorTheme: progressIndicatorTheme),
    );
  }
}
