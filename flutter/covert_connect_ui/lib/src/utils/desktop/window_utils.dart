import 'dart:io';
import 'dart:ui';

import 'package:flutter_acrylic/flutter_acrylic.dart';
import 'package:shared_preferences/shared_preferences.dart';

Brightness? _lastBrightness;
Future<void> updateEffect(Brightness? brightness) async {
  final useBrightness = brightness ?? _lastBrightness;
  if (useBrightness == null) return;

  _lastBrightness = useBrightness;
  await Window.setEffect(effect: Platform.isLinux ? WindowEffect.solid : WindowEffect.mica, dark: useBrightness == Brightness.dark);
}

class WindowState {
  const WindowState({required this.visible, this.position, this.size});

  static const String windowVisibleKey = 'window_visible';
  static const String windowPositionXKey = 'window_position_x';
  static const String windowPositionYKey = 'window_position_y';
  static const String windowSizeXKey = 'window_size_x';
  static const String windowSizeYKey = 'window_size_y';

  static Future<WindowState> load() async {
    final prefs = SharedPreferencesAsync();
    final posX = await prefs.getDouble(windowPositionXKey);
    final posY = await prefs.getDouble(windowPositionYKey);
    final position = posX != null && posY != null ? Offset(posX, posY) : null;
    final sizeX = await prefs.getDouble(windowSizeXKey);
    final sizeY = await prefs.getDouble(windowSizeYKey);
    final size = sizeX != null && sizeY != null ? Size(sizeX, sizeY) : null;
    return WindowState(visible: await prefs.getBool(windowVisibleKey) ?? true, position: position, size: size);
  }

  static Future<void> savePosition(Offset position) async {
    await SharedPreferencesAsync().setDouble(windowPositionXKey, position.dx);
    await SharedPreferencesAsync().setDouble(windowPositionYKey, position.dy);
  }

  static Future<void> saveSize(Size size) async {
    await SharedPreferencesAsync().setDouble(windowSizeXKey, size.width);
    await SharedPreferencesAsync().setDouble(windowSizeYKey, size.height);
  }

  static Future<void> saveVisible(bool visible) async {
    await SharedPreferencesAsync().setBool(windowVisibleKey, visible);
  }

  final bool visible;
  final Offset? position;
  final Size? size;
}
