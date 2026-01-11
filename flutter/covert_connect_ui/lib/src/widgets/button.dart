import 'package:covert_connect/src/widgets/hover.dart';
import 'package:flutter/material.dart';

class Button extends StatefulWidget {
  const Button({super.key, required this.label, this.onTap, this.primary = false, this.padding});

  final String label;
  final VoidCallback? onTap;
  final bool primary;
  final EdgeInsetsGeometry? padding;

  @override
  State<Button> createState() => _ButtonState();
}

class _ButtonState extends State<Button> {
  bool get primary => widget.primary;

  bool _hovering = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    Color? color;
    bool hovering = _hovering && widget.onTap != null;
    if (primary) {
      color = hovering ? Color.lerp(colorScheme.secondary, Colors.black, 0.33) : colorScheme.secondary;
    } else {
      color = hovering ? colorScheme.secondary.withValues(alpha: 0.2) : null;
    }

    Color outlineColor = colorScheme.outline;
    Color textColor = primary ? colorScheme.onSecondary : colorScheme.onSurface;
    if (widget.onTap == null) {
      final brightness = colorScheme.brightness;
      color = color?.withValues(alpha: brightness == Brightness.dark ? 0.1 : 0.47);
      textColor = primary
          ? textColor.withValues(alpha: brightness == Brightness.dark ? 0.1 : 0.47)
          : textColor.withValues(alpha: brightness == Brightness.dark ? 0.1 : 0.3);
    }

    final borderRadius = BorderRadius.circular(8);
    return Hover(
      onChange: (hovering) {
        setState(() {
          _hovering = hovering;
        });
      },
      child: Container(
        decoration: BoxDecoration(
          color: color,
          border: primary ? null : Border.all(color: outlineColor),
          borderRadius: borderRadius,
        ),
        child: ClipRRect(
          borderRadius: borderRadius,
          child: Material(
            color: Colors.transparent,
            child: InkWell(
              onTap: widget.onTap,
              child: Padding(
                padding: widget.padding ?? EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Text(widget.label, style: theme.textTheme.bodyMedium?.copyWith(color: textColor)),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
