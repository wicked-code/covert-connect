import 'package:covert_connect/src/widgets/desktop/caption_button_base.dart';
import 'package:flutter/material.dart';

const kWindowsButtonSize = Size(46, 32);

const kWinodwsCloseIconPressedColor = Color(0xFFEDBEBB);
const kWindowsCloseIconHoveredColor = Color(0xFFF8E4E3);

const kWindowsIconTheme = CaptionButtonIconColorTheme(
        normal: Color(0xFF171717),
        hovered: Color(0xFF000000),
        pressed: Color(0xFF5F5F5F),
        disabled: Color(0xFFB6B1A9),
        notFocused: Color(0xFF8B8B8B),
      );
const kWindowsIconThemeDark = CaptionButtonIconColorTheme(
        normal: Color(0xFFF2F2F2),
        hovered: Color(0xFFFFFFFF),
        pressed: Color(0xFFA7A7A7),
        disabled: Color(0xFF605C59),
        notFocused: Color(0xFF737373),
      );

class CaptionBackButtonWindows extends StatelessWidget {
  const CaptionBackButtonWindows({super.key, required this.focused, required this.brightness, this.onTap});

  final bool focused;
  final Brightness brightness;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return CaptionButtonBase(
      size: kWindowsButtonSize,
      focused: focused,
      brightness: brightness,
      onPressed: onTap,
      icon: "assets/icons/windows-back.svg",
      iconTheme: kWindowsIconTheme,
      iconThemeDark: kWindowsIconThemeDark,
      bgTheme: const CaptionButtonColorTheme(
        normal: Colors.transparent,
        hovered: Color(0x0B000000),
        pressed: Color(0x09000000),
      ),
      bgThemeDark: const CaptionButtonColorTheme(
        normal: Colors.transparent,
        hovered: Color(0x0BFFFFFF),
        pressed: Color(0x09FFFFFF),
      ),
    );
  }
}

class CaptionCloseButtonWindows extends StatelessWidget {
  const CaptionCloseButtonWindows({super.key, required this.focused, required this.brightness, this.onTap});

  final bool focused;
  final Brightness brightness;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return CaptionButtonBase(
      size: kWindowsButtonSize,
      focused: focused,
      brightness: brightness,
      onPressed: onTap,
      icon: "assets/icons/windows-x.svg",
      iconTheme: 
      kWindowsIconTheme.copyWith(pressed: kWinodwsCloseIconPressedColor, hovered: kWindowsCloseIconHoveredColor),
      iconThemeDark: kWindowsIconThemeDark.copyWith(pressed: kWinodwsCloseIconPressedColor, hovered: kWindowsCloseIconHoveredColor),
      bgTheme: const CaptionButtonColorTheme(
        normal: Colors.transparent,
        hovered: Color(0xFFC42B1C),
        pressed: Color(0xFFC83C30),
      ),
      bgThemeDark: const CaptionButtonColorTheme(
        normal: Colors.transparent,
        hovered: Color(0xFFC42B1C),
        pressed: Color(0xFFB3271C),
      ),
    );
  }
}

