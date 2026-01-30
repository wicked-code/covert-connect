import 'dart:async';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:flutter/material.dart';

const kReadChunkSize = 100;

const ansiReset = 0;
const ansiBold = 1;
const ansiFaint = 2;
const ansiItalic = 3;
const ansiUnderline = 4;
const ansiStrike = 9;
const ansiForegroundStart = 30;
const ansiForegroundEnd = 37;
const ansiForeground = 38;
const ansiDefaultForeground = 39;
const ansiBackgroundStart = 40;
const ansiBackgroundEnd = 47;
const ansiBackground = 48;
const ansiDefaultBackground = 49;
const ansiBrightForegroundStart = 90;
const ansiBrightForegroundEnd = 97;
const ansiBrightBackgroundStart = 100;
const ansiBrightBackgroundEnd = 107;
const ansiColorRGB = 2;
const ansiColorIndex = 5;

final RegExp regex = RegExp(r'\x1B\[([0-9;]+)m');

class LogPage extends StatefulWidget {
  const LogPage({super.key});

  @override
  State<LogPage> createState() => _LogPageState();
}

class _LogPageState extends State<LogPage> {
  late Timer _timer;
  final List<LogLine> _logMessages = [];
  bool _endReached = false;

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
          style = style.copyWith(color: Colors.grey[600], fontWeight: FontWeight.w300);
        } else if (code == ansiItalic) {
          style = style.copyWith(fontStyle: FontStyle.italic);
        } else if (code == ansiUnderline) {
          style = style.copyWith(decoration: TextDecoration.underline);
        } else if (code == ansiStrike) {
          style = style.copyWith(decoration: TextDecoration.lineThrough);
        } else if (code >= ansiForegroundStart && code <= ansiForegroundEnd) {
          style = style.copyWith(color: _basicColor(code - ansiForegroundStart, brightness));
        } else if (code == ansiForeground) {
          final (len, color) = _getColor(codes, index, brightness);
          if (len != null && color != null) {
            style = style.copyWith(color: color);
            index += len;
          }
        } else if (code == ansiDefaultForeground) {
          style = style.copyWith(color: null);
        } else if (code >= ansiBackgroundStart && code <= ansiBackgroundEnd) {
          style = style.copyWith(backgroundColor: _basicColor(code - ansiBackgroundStart, brightness));
        } else if (code == ansiBackground) {
          final (len, color) = _getColor(codes, index, brightness);
          if (len != null && color != null) {
            style = style.copyWith(backgroundColor: color);
            index += len;
          }
        } else if (code == ansiDefaultBackground) {
          style = style.copyWith(backgroundColor: null);
        } else if (code >= ansiBrightForegroundStart && code <= ansiBrightForegroundEnd) {
          style = style.copyWith(color: _brightColor(code - ansiBrightForegroundStart, brightness));
        } else if (code >= ansiBrightBackgroundStart && code <= ansiBrightBackgroundEnd) {
          style = style.copyWith(backgroundColor: _brightColor(code - ansiBrightBackgroundStart, brightness));
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

  (int? len, Color? color) _getColor(List<int> codes, int index, Brightness brightness) {
    int? len;
    Color? color;
    if (index + 1 < codes.length) {
      if (index + 4 < codes.length && codes[index + 1] == ansiColorRGB) {
        final r = codes[index + 2];
        final g = codes[index + 3];
        final b = codes[index + 4];
        color = Color.fromARGB(255, r, g, b);
        len = 4;
      } else if (index + 2 < codes.length && codes[index + 1] == ansiColorIndex) {
        color = _basicColor(codes[index + 2], brightness);
        len = 2;
      }
    }

    return (len, color != null ? updateBrightness(color, brightness) : null);
  }

  Color _basicColor(int index, Brightness brightness) {
    const colors = [
      Colors.black,
      Colors.red,
      Colors.green,
      Color(0xFFFDD835),
      Colors.blue,
      Colors.purple,
      Colors.cyan,
      Colors.white,
    ];
    return updateBrightness(colors[index], brightness);
  }

  Color _brightColor(int index, Brightness brightness) {
    const colors = [
      Colors.grey,
      Colors.redAccent,
      Colors.lightGreenAccent,
      Colors.yellowAccent,
      Colors.lightBlueAccent,
      Colors.pinkAccent,
      Colors.cyanAccent,
      Colors.white,
    ];
    return updateBrightness(colors[index], brightness);
  }

  Color updateBrightness(Color color, Brightness brightness) {
    return brightness == Brightness.dark ? color : darken(color, 0.7, 1.0, brightness);
  }

  void _loadMore() async {
    if (_endReached) return;

    final lastPosition = _logMessages.last.position;
    final newMessages = await di<ProxyServiceBase>().getLog(lastPosition, kReadChunkSize);
    if (newMessages.isEmpty) {
      _endReached = true;
      return;
    }

    _logMessages.addAll(newMessages);
    _updateIfMounted();
  }

  void _readLogNew() async {
    final List<LogLine> toAdd = [];
    final List<LogLine> toAddFull = [];
    final firstPos = _logMessages.isNotEmpty ? _logMessages.first.position : BigInt.from(-1);

    do {
      toAdd.clear();
      final start = toAddFull.lastOrNull?.position;
      final newMessages = (await di<ProxyServiceBase>().getLog(start, kReadChunkSize)).reversed.toList();
      if (start == null) {
        int pos = newMessages.length;
        while(pos > 0) {
          if (newMessages[pos - 1].line.isNotEmpty) {
            break;
          }
          pos --;
        }
        newMessages.removeRange(pos, newMessages.length);
      }

      for (int idx = newMessages.length - 1; idx >= 0; idx--) {
        final msg = newMessages[idx];
        if (msg.position > firstPos) {
          toAdd.add(msg);
        } else {
          break;
        }
      }
      toAddFull.addAll(toAdd);
    } while (toAdd.length == kReadChunkSize && _logMessages.isNotEmpty);
    _logMessages.insertAll(0, toAddFull);

    _updateIfMounted();
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    _readLogNew();
    _timer = Timer.periodic(Duration(seconds: 1), (timer) => _readLogNew());
    super.initState();
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final brightness = Theme.of(context).brightness;
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(8.0),
        child: CustomScrollView(
          reverse: true,
          shrinkWrap: true,
          slivers: <Widget>[
            SliverList(
              delegate: SliverChildBuilderDelegate((BuildContext context, int index) {
                if (index == _logMessages.length - 1) {
                  _loadMore();
                }

                return RichText(
                  text: TextSpan(
                    children: _parseAnsi(_logMessages[index].line, brightness),
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                );
              }, childCount: _logMessages.length),
            ),
          ],
        ),
      ),
    );
  }
}
