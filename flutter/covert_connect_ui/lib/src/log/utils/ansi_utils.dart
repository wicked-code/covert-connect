import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:flutter/material.dart';

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

(int? len, Color? color) getColor(List<int> codes, int index, Brightness brightness) {
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
      color = basicColor(codes[index + 2], brightness);
      len = 2;
    }
  }

  return (len, color != null ? _updateBrightness(color, brightness) : null);
}

Color basicColor(int index, Brightness brightness) {
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
  return _updateBrightness(colors[index], brightness);
}

Color brightColor(int index, Brightness brightness) {
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
  return _updateBrightness(colors[index], brightness);
}

Color _updateBrightness(Color color, Brightness brightness) {
  return brightness == Brightness.dark ? color : darken(color, 0.7, 1.0, brightness);
}
