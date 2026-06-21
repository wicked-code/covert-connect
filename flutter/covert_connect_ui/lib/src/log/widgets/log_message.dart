import 'package:covert_connect/src/log/utils/ansi_utils.dart';
import 'package:covert_connect/src/utils/text_width.dart';
import 'package:flutter/material.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:intl/intl.dart';

final thinTextStyle = GoogleFonts.inter(fontSize: 12, fontWeight: FontWeight.w300, height: 1.0);
final regexColors = RegExp(r'\x1B\[([0-9;]+)m');
final connectingMessageRegex = RegExp(r'^(.+?) connecting to ([^:]+):(\d+)(.*)?$', caseSensitive: false);
final directConnectingMessageRegex = RegExp(r'^(.+?) direct connecting to ([^:]+):(\d+)(.*)?$', caseSensitive: false);
final pathDelimeter = RegExp(r'[\\/]');

class LogMessage extends StatelessWidget {
  const LogMessage({super.key, required this.message});

  final LogMessageDto message;

  List<TextSpan> _parseAnsi(String text, Brightness brightness) {
    final List<TextSpan> spans = [];

    void addSpan(String text, TextStyle style) {
      spans.add(TextSpan(text: text, style: style));
    }

    void addSpanEx(String text, TextStyle style) {
      // check for connecting message
      bool direct = true;
      var match = directConnectingMessageRegex.firstMatch(text);
      if (match == null) {
        match = connectingMessageRegex.firstMatch(text);
        direct = false;
      }

      if (match == null) {
        addSpan(text, style);
        return;
      }

      final path = match.group(1);
      final domain = match.group(2);
      final port = match.group(3);
      final suffix = match.group(4);
      if (path == null || domain == null) {
        addSpan(text, style);
        return;
      }

      addSpan(path.substring(path.lastIndexOf(pathDelimeter) + 1), thinTextStyle);
      addSpan(direct ? ' => ' : ' -> ', style);
      addSpan(domain, thinTextStyle);
      if (port != null && port.isNotEmpty && port != "443") {
        addSpan(':$port', thinTextStyle);
      }
      if (suffix != null && suffix.isNotEmpty) {
        addSpan(" $suffix", thinTextStyle);
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
    final theme = Theme.of(context);
    final brightness = theme.brightness;
    final textStyle = theme.textTheme.bodySmall?.copyWith(overflow: TextOverflow.ellipsis);

    return LayoutBuilder(
      builder: (BuildContext ctx, BoxConstraints constraints) {
        final invertedBrightness = brightness == Brightness.dark ? Brightness.light : Brightness.dark;
        final span = TextSpan(children: _parseAnsi(message.message, brightness), style: textStyle);
        final spanWidth = calcSpanWidth(span, context);
        final overflow = spanWidth > constraints.maxWidth;

        return SelectionArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: DateFormat("HH:mm:ss.SSS ").format(message.timestamp.toLocal()),
                      style: thinTextStyle.copyWith(color: Colors.grey[600]),
                    ),
                    TextSpan(
                      text: message.level.name,
                      style: TextStyle(
                        color: switch (message.level) {
                          LogLevel.INFO => basicColor(2, brightness),
                          LogLevel.WARN => basicColor(3, brightness),
                          LogLevel.ERROR => basicColor(1, brightness),
                        },
                      ),
                    ),
                  ],
                  style: textStyle,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              if (!overflow) Text.rich(span, maxLines: 1, overflow: TextOverflow.ellipsis),
              if (overflow)
                Tooltip(
                  margin: const EdgeInsets.all(8),
                  richMessage: WidgetSpan(
                    child: RichText(
                      textWidthBasis: TextWidthBasis.longestLine,
                      text: TextSpan(
                        children: _parseAnsi(message.message, invertedBrightness),
                        style: textStyle?.copyWith(color: theme.colorScheme.onPrimary),
                      ),
                    ),
                  ),
                  child: Text.rich(span, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: TextAlign.start),
                ),
            ],
          ),
        );
      },
    );
  }
}
