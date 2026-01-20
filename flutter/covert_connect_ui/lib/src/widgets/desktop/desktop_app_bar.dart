import 'dart:io';
import 'dart:ui';

import 'package:covert_connect/di.dart';
import 'package:bitsdojo_window/bitsdojo_window.dart';
import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/utils/child_router.dart';
import 'package:covert_connect/src/utils/desktop/init.dart';
import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/utils/desktop/window_utils.dart';
import 'package:covert_connect/src/widgets/desktop/caption_button_base.dart';
import 'package:covert_connect/src/widgets/desktop/caption_buttons_macos.dart';
import 'package:covert_connect/src/widgets/desktop/caption_buttons_windows.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

const kTabHeight = 36.0;

class DesktopAppBar extends StatefulWidget {
  const DesktopAppBar({super.key, required this.tabController});

  final TabController tabController;

  @override
  State<DesktopAppBar> createState() => _DesktopAppBarState();
}

class _DesktopAppBarState extends State<DesktopAppBar>
    with WindowListener, TrayListener, AppRouteAware, SingleTickerProviderStateMixin {
  Brightness? _brightness;
  bool _isFocused = false;
  bool _canPop = false;

  late AnimationController _resizeAnimation;
  Size _resizeAnimationStartSize = Size.zero;

  void _initTray() async {
    if (!isDesktop) return;

    await _setIcons();

    Menu menu = Menu(
      items: [
        MenuItem(key: 'show_window', label: 'Show Window'),
        MenuItem.separator(),
        MenuItem(key: 'exit_app', label: 'Exit App'),
      ],
    );
    await trayManager.setContextMenu(menu);
  }

  Future<void> _setIcons() async {
    if (!isDesktop) return;

    if (Platform.isWindows) {
      final icon = _brightness == Brightness.dark ? "assets/images/app-icon-dark.ico" : "assets/images/app-icon.ico";
      await trayManager.setIcon(icon);
      await windowManager.setIcon(icon);
    } else {
      await trayManager.setIcon("assets/images/app-icon-dark.png");
    }
    await trayManager.setToolTip("Covert Connect");
  }

  void back() {
    appChildNavigator.pop();
  }

  void hide() {
    windowManager.getPosition().then((position) => WindowState.saveVisible(false));
    di<AppStateService>().value = AppState.hidden;
    appWindow.hide();
  }

  void resizeToDefault() async {
    WindowState.saveSize(kDefaultWindowSize);
    if (!Platform.isMacOS) {
      final size = await windowManager.getSize();
      if (mounted) {
        _resizeAnimationStartSize = size;
        _resizeAnimation.reset();
        _resizeAnimation.forward();
      }
    } else {
      windowManager.setSize(kDefaultWindowSize, animate: true);
    }
  }

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    if (previousRoute == null) return;
    _canPop = true;
    setState(() {});
  }

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {
    _canPop = appChildNavigator.canPop();
    setState(() {});
  }

  @override
  void didChangeNavigator(NavigatorState? navigator) {
    _canPop = navigator?.canPop() ?? false;
    setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _resizeAnimation = AnimationController(duration: Durations.short3, vsync: this);
    final curvedAnimation = CurvedAnimation(parent: _resizeAnimation, curve: Curves.easeInOut);
    curvedAnimation.addListener(() async {
      if (_resizeAnimationStartSize == Size.zero) return;
      final scale = appWindow.scaleFactor;

      final pos = await windowManager.getPosition();
      appWindow.rect = Rect.fromLTWH(pos.dx * scale, pos.dy * scale, 
          (_resizeAnimationStartSize.width +
              (kDefaultWindowSize.width - _resizeAnimationStartSize.width) * curvedAnimation.value) * scale,
          (_resizeAnimationStartSize.height +
              (kDefaultWindowSize.height - _resizeAnimationStartSize.height) * curvedAnimation.value) * scale,
      );
    });

    appChildNavigator.subscribe(this);
    trayManager.addListener(this);
    windowManager.addListener(this);
    windowManager.isFocused().then((isFocused) => setState(() => _isFocused = isFocused));
  }

  @override
  void dispose() {
    appChildNavigator.unsubscribe(this);
    windowManager.removeListener(this);
    trayManager.removeListener(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final brightness = MediaQuery.of(context).platformBrightness;
    if (isDesktop) {
      if (_brightness == null) {
        _brightness = brightness;
        _initTray();
        updateEffect(brightness);
      } else if (_brightness != brightness) {
        _brightness = brightness;
        _setIcons();
        updateEffect(brightness);
      }
    }

    CaptionButtonIconColorTheme theme = Platform.isMacOS
        ? brightness == Brightness.dark
              ? kMacOSIconThemeDark
              : kMacOSIconTheme
        : brightness == Brightness.dark
        ? kWindowsIconThemeDark
        : kWindowsIconTheme;
    Color captionColor = theme.normal;
    if (!_isFocused) {
      captionColor = theme.notFocused;
    }

    return Column(
      children: [
        SizedBox(
          height: Platform.isMacOS ? 28 : 32,
          child: _MoveWindow(
            onDoubleTap: resizeToDefault,
            child: Platform.isMacOS
                ? _TitleBarMacOs(
                    brightness: brightness,
                    isFocused: _isFocused,
                    color: captionColor,
                    onTapClose: hide,
                    onTapBack: _canPop ? back : null,
                    onDoubleTap: resizeToDefault,
                  )
                : _TitleBarWindows(
                    brightness: brightness,
                    isFocused: _isFocused,
                    color: captionColor,
                    onTapClose: hide,
                    onTapBack: _canPop ? back : null,
                    onDoubleTap: resizeToDefault,
                  ),
          ),
        ),
        Stack(
          children: [
            Positioned(
              left: 0,
              right: 0,
              bottom: 0,
              child: Container(height: 1, color: Theme.of(context).dividerColor),
            ),
            Center(
              child: TabBar(
                controller: widget.tabController,
                splashBorderRadius: BorderRadius.only(topLeft: Radius.circular(8), topRight: Radius.circular(8)),
                tabAlignment: TabAlignment.center,
                dividerColor: Colors.transparent,
                tabs: [
                  Tab(height: kTabHeight, child: Text("Status")),
                  Tab(height: kTabHeight, child: Text("Domains")),
                  Tab(height: kTabHeight, child: Text("Log")),
                  Tab(height: kTabHeight, child: Text("Options")),
                ],
              ),
            ),
          ],
        ),
      ],
    );
  }

  @override
  void onWindowMoved() {
    windowManager.getPosition().then((position) {
      WindowState.savePosition(position);
    });
  }

  @override
  void onWindowResize() {
    windowManager.getSize().then((size) {
      WindowState.saveSize(size);
    });
  }

  @override
  void onWindowBlur() {
    if (mounted) setState(() => _isFocused = false);
  }

  @override
  void onWindowFocus() {
    if (mounted) setState(() => _isFocused = true);
    windowManager.getPosition().then((position) => WindowState.saveVisible(true));
    di<AppStateService>().value = AppState.visible;
  }

  @override
  void onTrayIconMouseDown() {
    if (Platform.isWindows) {
      _showWindow();
    } else {
      trayManager.popUpContextMenu();
    }
  }

  @override
  void onTrayIconRightMouseDown() {
    if (Platform.isWindows) {
      trayManager.popUpContextMenu();
    }
  }

  @override
  void onTrayMenuItemClick(MenuItem menuItem) async {
    if (menuItem.key == 'show_window') {
      _showWindow();
    } else if (menuItem.key == 'exit_app') {
      appWindow.close();
      if (Platform.isMacOS) {
        ServicesBinding.instance.exitApplication(AppExitType.required);
      }
    }
  }

  void _showWindow() {
    appWindow.show();
    if (appWindow.isVisible) {
      windowManager.focus();
    }
  }
}

class _TitleBarWindows extends StatelessWidget {
  const _TitleBarWindows({
    required this.brightness,
    required this.isFocused,
    required this.color,
    required this.onTapClose,
    required this.onTapBack,
    required this.onDoubleTap,
  });

  final Brightness brightness;
  final bool isFocused;
  final Color color;
  final VoidCallback? onTapBack;
  final VoidCallback onTapClose;
  final VoidCallback onDoubleTap;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        CaptionBackButtonWindows(focused: isFocused, brightness: brightness, onTap: onTapBack),
        Expanded(
          child: GestureDetector(
            onDoubleTap: onDoubleTap,
            behavior: HitTestBehavior.opaque,
            child: Center(
              child: SizedBox(height: 22, width: 22, child: buildSvg("assets/icons/app-icon.svg", color: color)),
              // TODO: ??? show icon crossed out if turned off (the same for tray and app icon!)
            ),
          ),
        ),
        CaptionCloseButtonWindows(focused: isFocused, brightness: brightness, onTap: onTapClose),
      ],
    );
  }
}

class _TitleBarMacOs extends StatefulWidget {
  const _TitleBarMacOs({
    required this.brightness,
    required this.isFocused,
    required this.color,
    required this.onTapClose,
    required this.onTapBack,
    required this.onDoubleTap,
  });

  final Brightness brightness;
  final bool isFocused;
  final Color color;
  final VoidCallback? onTapBack;
  final VoidCallback onTapClose;
  final VoidCallback onDoubleTap;

  @override
  State<_TitleBarMacOs> createState() => _TitleBarMacOsState();
}

class _TitleBarMacOsState extends State<_TitleBarMacOs> {
  Brightness get brightness => widget.brightness;
  bool get isFocused => widget.isFocused;

  bool _isHovered = false;
  void _onEntered({required bool hovered}) {
    setState(() => _isHovered = hovered);
  }

  @override
  Widget build(BuildContext context) {
    const kSpacing = 8.0;
    return Row(
      spacing: kSpacing,
      children: [
        SizedBox(width: 1),
        MouseRegion(
          onExit: (value) => _onEntered(hovered: false),
          onHover: (value) => _onEntered(hovered: true),
          child: Row(
            spacing: kSpacing,
            children: [
              CaptionCloseButtonMacOs(
                focused: isFocused,
                hovered: _isHovered,
                brightness: brightness,
                onTap: widget.onTapClose,
              ),
              CaptionDisabledButtonMacOs(brightness: brightness),
              CaptionDisabledButtonMacOs(brightness: brightness),
            ],
          ),
        ),
        CaptionBackButtonMacOS(focused: isFocused, brightness: brightness, onTap: widget.onTapBack),
        Expanded(
          child: GestureDetector(
            onDoubleTap: widget.onDoubleTap,
            behavior: HitTestBehavior.opaque,
            child: Row(
              children: [
                // TODO: ??? show icon crossed out if turned off (the same for tray and app icon!)
                SizedBox(width: kSpacing),
                Text("Covert Connect", style: TextStyle(color: widget.color)),
                //SizedBox(height: 22, width: 22, child: buildSvg("assets/icons/app-icon.svg", color: widget.color)),
                Expanded(child: Container()),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _MoveWindow extends StatelessWidget {
  const _MoveWindow({this.child, this.onDoubleTap});

  final Widget? child;
  final VoidCallback? onDoubleTap;
  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onPanStart: (details) {
        appWindow.startDragging();
      },
      child: child ?? Container(),
    );
  }
}
