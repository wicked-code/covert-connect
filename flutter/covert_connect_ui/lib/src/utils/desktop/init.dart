import 'dart:developer';
import 'dart:io';

import 'package:bitsdojo_window/bitsdojo_window.dart';
import 'package:covert_connect/src/utils/desktop/window_utils.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:flutter/material.dart';
import 'package:flutter_acrylic/window.dart';
import 'package:flutter_single_instance/flutter_single_instance.dart';
import 'package:windows_single_instance/windows_single_instance.dart';

const kDefaultWindowSize = Size(400, 600);

Future<void> initDesktop(List<String> args) async {
  if (!isDesktop) return;

  final String instanceId = Platform.resolvedExecutable.replaceAll(RegExp(r'[^a-zA-Z0-9_-]'), '_');
  if (Platform.isWindows) {
    await WindowsSingleInstance.ensureSingleInstance(
      args,
      instanceId,
      onSecondWindow: (args) {
        if (args.contains('/exit')) {
          exit(0);
        }
      },
    );
  } else {
    FlutterSingleInstance.debugMode = false;
    FlutterSingleInstance.processName = instanceId;
    if (await FlutterSingleInstance().isFirstInstance() == false) {
      final err = await FlutterSingleInstance().focus({"args": args});
      if (err != null) {
        log("Error focusing running instance: $err");
      }

      exit(0);
    }

    FlutterSingleInstance.onFocus = (data) {
      if ((data['args'] as List?)?.contains('/exit') ?? false) {
        exit(0);
      }
    };
  }
  if (args.contains('/exit')) {
    exit(0);
  }

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

    const kMinSize = Size(360, 540);
    const kMaxSize = Size(480, 900);
    if (windowSize.width < kMinSize.width || windowSize.height < kMinSize.height) {
      windowSize = kMinSize;
    } else if (windowSize.width > kMaxSize.width || windowSize.height > kMaxSize.height) {
      windowSize = kMaxSize;
    }

    appWindow.size = windowSize;
    appWindow.minSize = kMinSize;
    appWindow.maxSize = kMaxSize;
    appWindow.alignment = Alignment.center;
    if (windowState.position != null) {
      Offset position = windowState.position!;
      if (Platform.isMacOS) {
        position += Offset(0, -appWindow.titleBarHeight);
      }
      final scale = Platform.isLinux ? 1.0 : appWindow.scaleFactor;
      appWindow.rect = Rect.fromLTWH(
        position.dx * scale,
        position.dy * scale,
        windowSize.width * scale,
        windowSize.height * scale,
      );
    }
    appWindow.show();
  });
}
