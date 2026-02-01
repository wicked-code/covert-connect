import 'dart:io';

import 'package:covert_connect/src/log/utils/ansi_utils.dart';
import 'package:flutter/material.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:intl/intl.dart';

final thinTextStyle = GoogleFonts.inter(fontSize: 12, fontWeight: FontWeight.w300, height: 1.0);
final RegExp regexColors = RegExp(r'\x1B\[([0-9;]+)m');
final RegExp pathRegexWin = RegExp(r'([a-zA-Z]:[\\/](?:[^\\/:*?"<>|\r\n]+[\\/])*[^\\/:*?"<>|\r\n]*\.?\w*)', caseSensitive: false);
final RegExp pathRegexUnix = RegExp(r'(/(?:[^/\0]+/)*[^/\0]*\.?\w*)');

class LogMessage extends StatelessWidget {
  const LogMessage({super.key, required this.message});

  final LogMessageDto message;

  List<TextSpan> _parseAnsi(String text, Brightness brightness) {
    final List<TextSpan> spans = [];

    void addSpan(String text, TextStyle style) {
      spans.add(TextSpan(text: text, style: style));
    }

    String replaceConnecting(String value) => value.replaceAll("connecting to", "->");

    void addSpanEx(String text, TextStyle style) {
      int lastIndex = 0;
      final regex = Platform.isWindows ? pathRegexWin : pathRegexUnix;
      for (final match in regex.allMatches(text)) {
        final prefix = text.substring(lastIndex, match.start);
        if (prefix.isNotEmpty) {
          addSpan(prefix, style);
        }

        final name = match.group(0)!.split(r'/').last.split(r'\').last;
        addSpan(replaceConnecting(name), thinTextStyle);
        lastIndex = match.end;
      }

      if (lastIndex < text.length) {
        addSpan(text.substring(lastIndex), style);
      }
    }

    int lastIndex = 0;
    TextStyle style = const TextStyle();

    for (final match in regexColors.allMatches(text)) {
      final prefix = text.substring(lastIndex, match.start);
      if (prefix.isNotEmpty) {
        addSpanEx(prefix, style);
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
      addSpanEx(text.substring(lastIndex), style);
    }

    return spans;
  }

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.of(context).size.width;
    final isSmall = width < 430;
    final brightness = Theme.of(context).brightness;
    return SelectableText.rich(
      TextSpan(
        children: [
          TextSpan(
            text: DateFormat("HH:mm:ss${isSmall ? '.SSS' : ''} ").format(message.timestamp),
            style: thinTextStyle.copyWith(color: Colors.grey[600]),
          ),
          TextSpan(
            text: "${message.level.name}${isSmall ? '\n' : ' '}",
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
