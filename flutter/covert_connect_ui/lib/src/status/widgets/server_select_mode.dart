import 'dart:async';
import 'dart:math';

import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/extensions.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

enum ServerSelectMode { multiselect, toggle }

class ServerSelectModeController extends ChangeNotifier {
  static const kModeKey = "server_select_mode";

  ServerSelectModeController({ServerSelectMode defaultMode = ServerSelectMode.multiselect}) : _mode = defaultMode {
    _init();
  }

  ServerSelectMode _mode;
  ServerSelectMode get mode => _mode;

  set mode(ServerSelectMode mode) {
    _mode = mode;
    _save();
    notifyListeners();
  }

  void _init() async {
    final prefs = SharedPreferencesAsync();
    final modeStr = await prefs.getString(kModeKey);
    if (modeStr != null) {
      mode = ServerSelectMode.values.byName(modeStr);
    }
  }

  void _save() async {
    final prefs = SharedPreferencesAsync();
    prefs.setString(kModeKey, mode.name);
  }
}

class ServerSelectModeMenu extends StatefulWidget {
  const ServerSelectModeMenu({super.key, required this.controller});

  final ServerSelectModeController controller;

  @override
  State<ServerSelectModeMenu> createState() => _ServerSelectModeMenuState();
}

class _ServerSelectModeMenuState extends State<ServerSelectModeMenu> {
  ServerSelectModeController get controller => widget.controller;

  static const kLeftShift = -5.0;

  void showMenu() {
    if (!mounted) return;

    final RenderBox anchorBox = context.findRenderObject()! as RenderBox;
    final bottomLeft = anchorBox.localToGlobal(anchorBox.size.bottomLeft(Offset.zero));
    final width = anchorBox.size.width;

    showDialog(
      barrierColor: isDesktop ? Colors.transparent : null,
      context: context,
      builder: (context) {
        final theme = Theme.of(context);
        final colorScheme = theme.colorScheme;

        void select(ServerSelectMode mode) {
          controller.mode = mode;
          Timer(Durations.short2, () => Navigator.pop(context));
        }

        final borderRadus = BorderRadius.circular(8);
        return Align(
          alignment: Alignment.topLeft,
          child: Container(
            margin: EdgeInsets.only(left: max(0, bottomLeft.dx + kLeftShift), top: bottomLeft.dy),
            decoration: BoxDecoration(
              borderRadius: borderRadus,
              color: colorScheme.surface,
              border: isDesktop ? null : Border.all(color: theme.dividerColor, width: 1.0),
              boxShadow: [
                if (isDesktop)
                  BoxShadow(color: Colors.black.withValues(alpha: 0.5), blurRadius: 10, offset: Offset(6, 6)),
              ],
            ),
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                if (isDesktop)
                  Positioned.fill(
                    child: CustomPaint(
                      foregroundPainter: PointerPainter(
                        xOffset: (width / 2) - kLeftShift,
                        bgColor: colorScheme.surface,
                      ),
                    ),
                  ),
                ClipRRect(
                  borderRadius: borderRadus,
                  child: IntrinsicWidth(
                    child: ListenableBuilder(
                      listenable: controller,
                      builder: (context, child) => Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          ...ServerSelectMode.values.map(
                            (mode) => _MenuItem(
                              text: mode.name.capitalized,
                              selected: mode == controller.mode,
                              onTap: () => select(mode),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                if (isDesktop)
                  Positioned.fill(
                    child: CustomPaint(
                      foregroundPainter: PointerPainter(
                        xOffset: (width / 2) - kLeftShift,
                        bgColor: colorScheme.surface,
                        borderColor: Color.alphaBlend(theme.dividerColor, colorScheme.surface),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, child) => AppIconButton(
        asset: switch (controller.mode) {
          ServerSelectMode.multiselect => "assets/icons/srv-sel-mul.svg",
          ServerSelectMode.toggle => "assets/icons/srv-sel-one.svg",
        },
        onPressed: showMenu,
      ),
    );
  }
}

class _MenuItem extends StatelessWidget {
  const _MenuItem({required this.text, required this.selected, required this.onTap});

  final String text;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    FontWeight? fontWeight;
    if ((theme.brightness == Brightness.dark && selected) || (theme.brightness == Brightness.light && !selected)) {
      fontWeight = FontWeight.w600;
    }
    bool hovered = false;
    return StatefulBuilder(
      builder: (context, setState) {
        var color = selected ? theme.colorScheme.primary : theme.colorScheme.surface;
        if (hovered) {
          color = darken(color, 1.2, 1.2, theme.brightness);
        }
        return MouseRegion(
          onExit: (value) => setState(() => hovered = false),
          onHover: (value) => setState(() => hovered = true),
          child: GestureDetector(
            onTap: onTap,
            child: Container(
              color: color,
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Center(
                child: Text(
                  text,
                  style: TextStyle(
                    color: selected ? theme.colorScheme.onPrimary : theme.colorScheme.onSurface,
                    fontWeight: fontWeight,
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class PointerPainter extends CustomPainter {
  PointerPainter({required this.xOffset, this.bgColor, this.borderColor});

  final Color? bgColor;
  final Color? borderColor;
  final double xOffset;

  static const kDiamondHalfSize = 6.0;
  static const kDiamondSize = kDiamondHalfSize * 2;
  static const kBorderRadius = 6.0;
  static const kBorderDia = kBorderRadius * 2;

  @override
  void paint(Canvas canvas, Size size) {
    assert((borderColor != null && bgColor != null) || (bgColor != null && borderColor == null));

    final rectRight = size.width - kBorderDia;
    final rectBottom = size.height - kBorderDia;
    final path = Path()
      ..moveTo(xOffset - kDiamondHalfSize, 0)
      ..lineTo(xOffset, -kDiamondHalfSize)
      ..lineTo(xOffset + kDiamondHalfSize, 0)
      ..lineTo(size.width - kBorderRadius, 0)
      ..arcTo(Rect.fromLTWH(rectRight, 0, kBorderDia, kBorderDia), -pi / 2, pi / 2, false)
      ..lineTo(size.width, size.height - kBorderRadius)
      ..arcTo(Rect.fromLTWH(rectRight, rectBottom, kBorderDia, kBorderDia), 2 * pi, pi / 2, false)
      ..lineTo(kBorderRadius, size.height)
      ..arcTo(Rect.fromLTWH(0, rectBottom, kBorderDia, kBorderDia), pi / 2, pi / 2, false)
      ..lineTo(0, kBorderRadius)
      ..arcTo(Rect.fromLTWH(0, 0, kBorderDia, kBorderDia), pi, pi / 2, false)
      ..close();

    if (borderColor != null) {
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeJoin = StrokeJoin.round
        ..strokeCap = StrokeCap.round
        ..strokeWidth = 1
        ..color = bgColor!;

      final pathBgLine = Path()
        ..moveTo(xOffset - kDiamondHalfSize, 0)
        ..lineTo(xOffset + kDiamondHalfSize, 0);
      canvas.drawPath(pathBgLine, paint);

      paint.color = borderColor!;
      canvas.drawPath(path, paint);
    } else {
      final paint = Paint()
        ..color = bgColor!
        ..style = PaintingStyle.fill;

      canvas.drawPath(path, paint);
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) {
    return false;
  }
}
