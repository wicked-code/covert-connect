import 'dart:ui';

import 'package:flutter/material.dart';

Color darken(Color color, double factor, double factorDark, Brightness brightness) {
  double lightnessFactor = brightness == Brightness.light ? factor : factorDark;
  HSLColor hslSurface = HSLColor.fromColor(color);
  return hslSurface.withLightness(clampDouble(hslSurface.lightness * lightnessFactor, 0.0, 1.0)).toColor();
}
