import 'dart:io';

import 'package:bitsdojo_window/bitsdojo_window.dart';
import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/utils/desktop/window_utils.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:flutter/material.dart';
import 'package:flutter_acrylic/window.dart';
import 'package:window_manager/window_manager.dart';
import 'package:covert_connect/di.dart';

const kDefaultWindowSize = Size(400, 600);

Future<void> initDesktop() async {
  if (!isDesktop) return;

  await windowManager.ensureInitialized();
  await Window.initialize();
  if (!Platform.isLinux) {
    await Window.hideWindowControls();
  }

  final windowState = await WindowState.load();

  doWhenWindowReady(() async {
    Size windowSize = windowState.size ?? kDefaultWindowSize;
    if (Platform.isMacOS) {
      windowSize += Offset(0, appWindow.titleBarHeight);
    }

    appWindow.size = windowSize;
    appWindow.minSize = const Size(360, 540);
    appWindow.maxSize = const Size(480, 900);
    appWindow.alignment = Alignment.center;
    if (windowState.position != null) {
      Offset position = windowState.position!;
      if (Platform.isMacOS) {
        position += Offset(0, -appWindow.titleBarHeight);
      }
      final scale = appWindow.scaleFactor;
      appWindow.rect = Rect.fromLTWH(
        position.dx * scale,
        position.dy * scale,
        windowSize.width * scale,
        windowSize.height * scale,
      );
    }
    if (windowState.visible) {
      appWindow.show();
      di<AppStateService>().value = AppState.visible;
    }
  });
}
