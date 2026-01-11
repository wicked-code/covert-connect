import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

class DesktopIconButton extends StatefulWidget {
  const DesktopIconButton({super.key, required this.icon, this.onPressed});

  final Widget icon;
  final VoidCallback? onPressed;

  @override
  State<DesktopIconButton> createState() => _DesktopIconButtonState();
}

class _DesktopIconButtonState extends State<DesktopIconButton> {
  Widget get icon => widget.icon;
  VoidCallback? get onPressed => widget.onPressed;

  bool isDown = false;
  bool isHovering = false;

  void handleMouseEnter(PointerEnterEvent event) {
    setState(() {
      isHovering = true;
    });
  }

  void handleMouseExit(PointerExitEvent event) {
    setState(() {
      isDown = false;
      isHovering = false;
    });
  }

  void handleTapDown(TapDownDetails details) {
    setState(() {
      isDown = true;
    });
  }

  void handleTapUp(TapUpDetails details) {
    setState(() {
      isDown = false;
    });
    onPressed?.call();
  }

  @override
  Widget build(BuildContext context) {
    Widget child;
    if (onPressed == null) {
      child = Opacity(
        key: ValueKey("disabled"),
        opacity: 0.33,
        child: ColorFiltered(
          colorFilter: ColorFilter.mode(
            Theme.of(context).colorScheme.primary.withValues(alpha: 0.67),
            BlendMode.srcATop,
          ),
          child: icon,
        ),
      );
    } else if (isHovering) {
      child = ColorFiltered(
        key: ValueKey("hover"),
        colorFilter: ColorFilter.mode(Theme.of(context).colorScheme.primary.withValues(alpha: 0.21), BlendMode.srcATop),
        child: icon,
      );
    } else {
      child = Container(key: ValueKey("child"), child: icon);
    }

    double scale = 1.0;
    if (onPressed != null) {
      scale = isDown ? 0.9 : (isHovering ? 1.11 : 1.0);
    }

    return MouseRegion(
      cursor: WidgetStateMouseCursor.clickable,
      onEnter: handleMouseEnter,
      onExit: handleMouseExit,
      child: GestureDetector(
        onTapDown: handleTapDown,
        onTapUp: handleTapUp,
        behavior: HitTestBehavior.opaque,
        child: AnimatedContainer(
          duration: Durations.short1,
          transform: Matrix4.identity()..scaleByDouble(scale, scale, scale, 1.0),
          transformAlignment: Alignment.center,
          width: 28,
          height: 28,
          child: Center(
            child: AnimatedSwitcher(
              duration: Durations.short2,
              // TODO: fix https://github.com/flutter/flutter/issues/121336
              transitionBuilder: (Widget child, Animation<double> animation) =>
                  FadeTransition(opacity: animation, child: child),
              //
              child: child,
            ),
          ),
        ),
      ),
    );
  }
}
