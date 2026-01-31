import 'package:covert_connect/src/log/utils/ansi_utils.dart';
import 'package:flutter/material.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:intl/intl.dart';

final thinTextStyle = GoogleFonts.inter(fontSize: 12, fontWeight: FontWeight.w300, height: 1.0);

class LogMessage extends StatelessWidget {
  const LogMessage({super.key, required this.message});

  final LogMessageDto message;

  List<TextSpan> _parseAnsi(String text, Brightness brightness) {
    final List<TextSpan> spans = [];

    int lastIndex = 0;
    TextStyle style = const TextStyle();

    for (final match in regex.allMatches(text)) {
      final prefix = text.substring(lastIndex, match.start);
      if (prefix.isNotEmpty) {
        spans.add(TextSpan(text: prefix, style: style));
      }

      final codes = match.group(1)!.split(';').map(int.parse).toList();
      for (int index = 0; index < codes.length; index++) {
        final code = codes[index];
        if (code == ansiReset) {
          style = const TextStyle();
        } else if (code == ansiBold) {
          style = style.copyWith(fontWeight: FontWeight.bold);
        } else if (code == ansiFaint) {
          style = thinTextStyle.copyWith(color: Colors.grey[600]);
        } else if (code == ansiItalic) {
          style = style.copyWith(fontStyle: FontStyle.italic);
        } else if (code == ansiUnderline) {
          style = style.copyWith(decoration: TextDecoration.underline);
        } else if (code == ansiStrike) {
          style = style.copyWith(decoration: TextDecoration.lineThrough);
        } else if (code >= ansiForegroundStart && code <= ansiForegroundEnd) {
          style = style.copyWith(color: basicColor(code - ansiForegroundStart, brightness));
        } else if (code == ansiForeground) {
          final (len, color) = getColor(codes, index, brightness);
          if (len != null && color != null) {
            style = style.copyWith(color: color);
            index += len;
          }
        } else if (code == ansiDefaultForeground) {
          style = style.copyWith(color: null);
        } else if (code >= ansiBackgroundStart && code <= ansiBackgroundEnd) {
          style = style.copyWith(backgroundColor: basicColor(code - ansiBackgroundStart, brightness));
        } else if (code == ansiBackground) {
          final (len, color) = getColor(codes, index, brightness);
          if (len != null && color != null) {
            style = style.copyWith(backgroundColor: color);
            index += len;
          }
        } else if (code == ansiDefaultBackground) {
          style = style.copyWith(backgroundColor: null);
        } else if (code >= ansiBrightForegroundStart && code <= ansiBrightForegroundEnd) {
          style = style.copyWith(color: brightColor(code - ansiBrightForegroundStart, brightness));
        } else if (code >= ansiBrightBackgroundStart && code <= ansiBrightBackgroundEnd) {
          style = style.copyWith(backgroundColor: brightColor(code - ansiBrightBackgroundStart, brightness));
        }
      }

      lastIndex = match.end;
    }

    // Add remaining text
    if (lastIndex < text.length) {
      spans.add(TextSpan(text: text.substring(lastIndex), style: style));
    }

    return spans;
  }

  @override
  Widget build(BuildContext context) {
    final brightness = Theme.of(context).brightness;
    return SelectableText.rich(
      TextSpan(
        children: [
          TextSpan(
            text: DateFormat('HH:mm:ss ').format(message.timestamp),
            style: thinTextStyle.copyWith(color: Colors.grey[600]),
          ),
          TextSpan(
            text: "${message.level.name} ",
            style: TextStyle(
              color: switch (message.level) {
                LogLevel.INFO => basicColor(2, brightness),
                LogLevel.WARN => basicColor(3, brightness),
                LogLevel.ERROR => basicColor(1, brightness),
              },
            ),
          ),
          ..._parseAnsi(message.message, brightness),
        ],
        style: Theme.of(context).textTheme.bodySmall,
      ),
    );
  }
}
