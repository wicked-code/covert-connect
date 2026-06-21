import 'package:flutter/material.dart';

double calcTextWidth(String text, BuildContext context, TextStyle textStyle) {
  final DefaultTextStyle defaultTextStyle = DefaultTextStyle.of(context);
  TextStyle effectiveTextStyle = defaultTextStyle.style.merge(textStyle);

  return calcSpanWidth(TextSpan(text: text, style: effectiveTextStyle), context);
}

double calcSpanWidth(InlineSpan? text, BuildContext context) {
  final TextPainter textPainter = TextPainter(
    text: text,
    textScaler: MediaQuery.textScalerOf(context),
    maxLines: 1,
    textDirection: TextDirection.ltr,
  )..layout(minWidth: 0, maxWidth: double.infinity);

  return textPainter.size.width;
}
