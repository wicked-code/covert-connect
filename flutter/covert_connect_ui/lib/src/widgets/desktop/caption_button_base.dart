import 'package:covert_connect/src/utils/svg.dart';
import 'package:flutter/material.dart';

class CaptionButtonColorTheme {
  const CaptionButtonColorTheme({required this.normal, required this.hovered, required this.pressed});
  final Color normal;
  final Color hovered;
  final Color pressed;
}

class CaptionButtonIconColorTheme extends CaptionButtonColorTheme {
  const CaptionButtonIconColorTheme({
    required super.normal,
    required super.hovered,
    required super.pressed,
    required this.disabled,
    required this.notFocused,
  });

  CaptionButtonIconColorTheme copyWith({
    Color? normal,
    Color? hovered,
    Color? pressed,
    Color? disabled,
    Color? notFocused,
  }) {
    return CaptionButtonIconColorTheme(
      normal: normal ?? this.normal,
      hovered: hovered ?? this.hovered,
      pressed: pressed ?? this.pressed,
      disabled: disabled ?? this.disabled,
      notFocused: notFocused ?? this.notFocused,
    );
  }

  final Color disabled;
  final Color notFocused;
}

class CaptionButtonBase extends StatefulWidget {
  const CaptionButtonBase({
    super.key,
    required this.focused,
    required this.brightness,
    required this.size,
    required this.icon,
    required this.bgTheme,
    required this.iconTheme,
    required this.bgThemeDark,
    required this.iconThemeDark,
    this.iconSize,
    this.onPressed,
  });

  final Size size;
  final Size? iconSize;
  final String icon;
  final bool focused;
  final Brightness brightness;
  final VoidCallback? onPressed;
  final CaptionButtonColorTheme bgTheme;
  final CaptionButtonIconColorTheme iconTheme;
  final CaptionButtonColorTheme bgThemeDark;
  final CaptionButtonIconColorTheme iconThemeDark;

  @override
  State<CaptionButtonBase> createState() => _CaptionButtonBaseState();
}

class _CaptionButtonBaseState extends State<CaptionButtonBase> {
  Brightness get brightness => widget.brightness;
  CaptionButtonColorTheme get bgTheme => brightness != Brightness.dark ? widget.bgTheme : widget.bgThemeDark;
  CaptionButtonIconColorTheme get iconTheme => brightness != Brightness.dark ? widget.iconTheme : widget.iconThemeDark;

  bool _isHovered = false;
  bool _isPressed = false;

  void _onEntered({required bool hovered}) {
    setState(() => _isHovered = hovered);
  }

  void _onActive({required bool pressed}) {
    setState(() => _isPressed = pressed);
  }

  @override
  Widget build(BuildContext context) {
    Color bgColor = bgTheme.normal;
    Color iconColor = iconTheme.normal;

    if (!widget.focused) {
      iconColor = iconTheme.notFocused;
    }
    if (_isHovered) {
      bgColor = bgTheme.hovered;
      iconColor = iconTheme.hovered;
    }
    if (_isPressed) {
      bgColor = bgTheme.pressed;
      iconColor = iconTheme.pressed;
    }
    if (widget.onPressed == null) {
      iconColor = iconTheme.disabled;
    }

    return MouseRegion(
      onExit: (value) => _onEntered(hovered: false),
      onHover: (value) => _onEntered(hovered: true),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (_) => _onActive(pressed: true),
        onTapCancel: () => _onActive(pressed: false),
        onTapUp: (_) => _onActive(pressed: false),
        onTap: widget.onPressed,
        child: Container(
          constraints: BoxConstraints(minWidth: widget.size.width, minHeight: widget.size.height),
          decoration: BoxDecoration(color: bgColor),
          child: Center(
            child: buildSvg(
              widget.icon,
              color: iconColor,
              width: widget.iconSize?.width,
              height: widget.iconSize?.height,
            ),
          ),
        ),
      ),
    );
  }
}
