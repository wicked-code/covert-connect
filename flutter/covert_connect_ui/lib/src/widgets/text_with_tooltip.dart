import 'package:covert_connect/src/utils/text_width.dart';
import 'package:flutter/material.dart';

class TextWithTooltip extends StatelessWidget {
  const TextWithTooltip(this.text, {super.key, this.tooltip, this.style});

  final String text;
  final String? tooltip;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) {
    final useTooltip = tooltip ?? text;
    final width = calcTextWidth(useTooltip, context, style ?? const TextStyle());

    return LayoutBuilder(
      builder: (BuildContext ctx, BoxConstraints constraints) {
        if (width < constraints.maxWidth) {
          return Text(text, overflow: TextOverflow.ellipsis, style: style);
        }

        return Tooltip(
          message: useTooltip,
          child: Text(text, overflow: TextOverflow.ellipsis, style: style),
        );
      },
    );
  }
}
