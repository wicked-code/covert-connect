import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:flutter/material.dart';

class ServerStateIndicator extends StatelessWidget {
  const ServerStateIndicator({super.key, required this.server, required this.onTap});

  final ServerInfo server;
  final VoidCallback onTap;

  static final kNotEnoughToCheckHealth = BigInt.two;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    Border? border;
    Color color = darken(theme.colorScheme.surface, 0.8, 1.33, theme.brightness);
    if (!server.config.enabled) {
      border = Border.all(color: color, width: 1);
      color = Colors.transparent;
    } else if (server.state.succesCount == BigInt.from(0)) {
      if (server.state.errCount > kNotEnoughToCheckHealth) {
        color = darken(Colors.red, 1.0, 0.9, theme.brightness);
      }
    } else if (server.state.errCount <= server.state.succesCount) {
      color = Colors.green;
    } else {
      color = darken(Colors.yellow, 0.67, 0.9, theme.brightness);
    }

    return AppIconButton(
      icon: Container(
        decoration: BoxDecoration(shape: BoxShape.circle, color: color, border: border),
        width: 11,
        height: 11,
      ),
      onPressed: onTap,
    );
  }
}
