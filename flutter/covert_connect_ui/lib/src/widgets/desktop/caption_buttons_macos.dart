import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/widgets/desktop/caption_button_base.dart';
import 'package:flutter/material.dart';

const kMacOSButtonSize = Size(28, 22);
const kCaptionButtonSize = Size(12, 12);

const kMacOSIconTheme = CaptionButtonIconColorTheme(
  normal: Color(0xFF5A5A59),
  hovered: Color(0xFF5A5A59),
  pressed: Color(0xFF3B3B3A),
  disabled: Color(0xFFB6B1A9),
  notFocused: Color(0xFF9B9B9B),
);
const kMacOSIconThemeDark = CaptionButtonIconColorTheme(
  normal: Color(0xFFEBEBEB),
  hovered: Color(0xFFB4B4B4),
  pressed: Color(0xFFECECEC),
  disabled: Color(0xFF605C59),
  notFocused: Color(0xFF737373),
);

class CaptionBackButtonMacOS extends StatelessWidget {
  const CaptionBackButtonMacOS({super.key, required this.focused, required this.brightness, this.onTap});

  final bool focused;
  final Brightness brightness;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(left: 8),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(6),
        child: SizedBox(
          width: kMacOSButtonSize.width,
          height: kMacOSButtonSize.height,
          child: CaptionButtonBase(
            size: kMacOSButtonSize,
            focused: focused,
            brightness: brightness,
            onPressed: onTap,
            icon: "assets/icons/chevron-left.svg",
            iconSize: const Size(12, 12),
            iconTheme: kMacOSIconTheme,
            iconThemeDark: kMacOSIconThemeDark,
            bgTheme: const CaptionButtonColorTheme(
              normal: Colors.transparent,
              hovered: Color(0x17171717),
              pressed: Color(0x33171717),
            ),
            bgThemeDark: const CaptionButtonColorTheme(
              normal: Colors.transparent,
              hovered: Color(0x17E5E4E3),
              pressed: Color(0x23E5E4E3),
            ),
          ),
        ),
      ),
    );
  }
}

class CaptionCloseButtonMacOs extends StatefulWidget {
  const CaptionCloseButtonMacOs({
    super.key,
    required this.focused,
    required this.hovered,
    required this.brightness,
    this.onTap,
  });

  final bool focused;
  final bool hovered;
  final Brightness brightness;
  final VoidCallback? onTap;

  @override
  State<CaptionCloseButtonMacOs> createState() => _CaptionCloseButtonMacOsState();
}

class _CaptionCloseButtonMacOsState extends State<CaptionCloseButtonMacOs> {
  bool get focused => widget.focused;
  bool get hovered => widget.hovered;
  Brightness get brightness => widget.brightness;

  bool _isPressed = false;
  void _onActive({required bool pressed}) {
    setState(() => _isPressed = pressed);
  }

  @override
  Widget build(BuildContext context) {
    if (!focused && !hovered) {
      return CaptionDisabledButtonMacOs(brightness: brightness);
    }

    final icon = Center(
      child: buildSvg(
        "assets/icons/macos-x.svg",
        width: 9,
        height: 9,
        color: brightness != Brightness.dark ? Color(0xFF780205) : Color(0xFF9C0205),
      ),
    );

    Color bgColor = Color(0xFFFF5F57);
    Color borderColor = Color(0xFFF2554E);
    if (_isPressed) {
      bgColor = brightness != Brightness.dark ? Color(0xFFD14E47) : Color(0xFFFF5A52);
      borderColor = brightness != Brightness.dark ? Color(0xFFC2443E) : Color(0xFFFA5851);
    }

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTapDown: (_) => _onActive(pressed: true),
      onTapCancel: () => _onActive(pressed: false),
      onTapUp: (_) => _onActive(pressed: false),
      onTap: widget.onTap,
      onPanStart: (_) {},
      child: Container(
        width: kCaptionButtonSize.width,
        height: kCaptionButtonSize.height,
        decoration: BoxDecoration(
          color: bgColor,
          border: Border.all(color: borderColor, width: 1),
          shape: BoxShape.circle,
        ),
        child: hovered ? icon : null,
      ),
    );
  }
}

class CaptionDisabledButtonMacOs extends StatelessWidget {
  const CaptionDisabledButtonMacOs({super.key, required this.brightness});

  final Brightness brightness;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: kCaptionButtonSize.width,
      height: kCaptionButtonSize.height,
      decoration: BoxDecoration(
        color: brightness != Brightness.dark ? Color(0xFFD7D6D5) : Color(0xFF4C4C4B),
        border: Border.all(color: brightness != Brightness.dark ? Color(0xFFC6C5C5) : Color(0xFF464645), width: 1),
        shape: BoxShape.circle,
      ),
    );
  }
}
