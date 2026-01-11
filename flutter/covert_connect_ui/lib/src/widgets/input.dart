import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class Input extends StatefulWidget {
  const Input({
    super.key,
    this.controller,
    this.onChanged,
    this.keyboardType,
    this.error = false,
    this.icon,
    this.padding,
    this.hint,
    this.textAlign,
    this.borderColor,
  });

  final TextEditingController? controller;
  final ValueChanged<String>? onChanged;
  final TextInputType? keyboardType;
  final bool error;
  final Widget? icon;
  final EdgeInsetsGeometry? padding;
  final String? hint;
  final TextAlign? textAlign;
  final Color? borderColor;

  @override
  State<Input> createState() => _InputState();
}

class _InputState extends State<Input> {
  final FocusNode _focusNode = FocusNode();
  bool _isHover = false;

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  @override
  void initState() {
    _focusNode.addListener(() {
      setState(() {});
    });
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final textStyle = theme.textTheme.bodyMedium;

    final background = (_focusNode.hasFocus || _isHover)
        ? colorScheme.surface
        : darken(colorScheme.surface, 0.95, 1.05, theme.brightness).withValues(alpha: 0.57);

    Color borderColor = _focusNode.hasFocus
        ? colorScheme.primary.withValues(alpha: 0.67)
        : Theme.of(context).dividerColor;
    if (widget.borderColor != null) {
      borderColor = widget.borderColor!;
    }
    if (widget.error) {
      borderColor = Colors.redAccent.withValues(alpha: 0.67);
    }

    return MouseRegion(
      onEnter: (_) => setState(() => _isHover = true),
      onExit: (_) => setState(() => _isHover = false),
      child: Container(
        padding: widget.padding ?? EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        decoration: BoxDecoration(
          color: background,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: borderColor, width: 1),
        ),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                focusNode: _focusNode,
                textAlign: widget.textAlign ?? TextAlign.start,
                keyboardType: widget.keyboardType,
                inputFormatters: widget.keyboardType == TextInputType.number
                    ? [FilteringTextInputFormatter.digitsOnly]
                    : null,
                onChanged: widget.onChanged,
                controller: widget.controller,
                minLines: 1,
                maxLines: 1,
                cursorHeight: textStyle != null ? textStyle.fontSize! * 1.3 : null,
                cursorWidth: 1,
                style: textStyle,
                decoration: InputDecoration(
                  hintText: widget.hint,
                  hintStyle: textStyle?.copyWith(color: textStyle.color?.withValues(alpha: 0.37)),
                  border: InputBorder.none,
                  isCollapsed: true,
                ),
              ),
            ),
            if (widget.icon != null) widget.icon!,
          ],
        ),
      ),
    );
  }
}
