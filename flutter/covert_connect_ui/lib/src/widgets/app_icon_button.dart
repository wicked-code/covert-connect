import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/desktop/desktop_icon_button.dart';
import 'package:flutter/material.dart';

class AppIconButton extends StatelessWidget {
  const AppIconButton({super.key, this.asset, this.assetColor, this.icon, this.onPressed})
    : assert((icon != null && asset == null) || (asset != null && icon == null));

  final String? asset;
  final Color? assetColor;
  final Widget? icon;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final useIcon = icon != null
        ? icon!
        : buildSvg(asset!, color: assetColor ?? Theme.of(context).colorScheme.secondary, height: 16);
    return isDesktop
        ? DesktopIconButton(onPressed: onPressed, icon: useIcon)
        : IconButton(onPressed: onPressed, icon: useIcon);
  }
}
