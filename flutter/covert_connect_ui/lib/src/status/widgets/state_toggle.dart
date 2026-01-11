import 'package:animated_toggle_switch/animated_toggle_switch.dart';
import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:flutter/material.dart';

class StateToggle extends StatefulWidget {
  const StateToggle({super.key});

  @override
  State<StateToggle> createState() => _StateToggleState();
}

class _StateToggleState extends State<StateToggle> {
  ProxyState? _state;

  void _setProxyState(ProxyState state) async {
    await di<ProxyServiceBase>().setProxyState(state);
    _update();
  }

  void _update() async {
    _state = await di<ProxyServiceBase>().getProxyState();
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    _update();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final dividerColor = theme.dividerColor;
    final indicatorSize = const Size(74.0, 36.0);
    final borderRadius = BorderRadius.circular(8);
    return Container(
      margin: EdgeInsets.symmetric(vertical: 16),
      padding: EdgeInsets.all(3),
      decoration: BoxDecoration(
        borderRadius: borderRadius,
        border: Border.all(color: dividerColor, width: 1),
        color: colorScheme.surface,
      ),
      child: AnimatedToggleSwitch<ProxyState>.size(
        active: _state != null,
        current: _state ?? ProxyState.off,
        values: [ProxyState.off, ProxyState.pac, ProxyState.all],
        borderWidth: 0,
        spacing: 2,
        iconOpacity: 0.67,
        selectedIconScale: 1.0,
        height: indicatorSize.height,
        indicatorSize: indicatorSize,
        iconAnimationType: AnimationType.onHover,
        styleAnimationType: AnimationType.onHover,
        style: ToggleStyle(borderColor: Colors.transparent, borderRadius: borderRadius),
        customSeparatorBuilder: (context, local, global) {
          final opacity = dividerColor.a * ((global.position - local.position).abs() - 0.5).clamp(0.0, 1.0);
          return VerticalDivider(indent: 10.0, endIndent: 10.0, color: dividerColor.withValues(alpha: opacity));
        },
        customIconBuilder: (context, local, global) {
          final name = switch (local.value) {
            ProxyState.off => 'OFF',
            ProxyState.pac => 'SMART',
            ProxyState.all => 'ALL',
          };
          return Transform.scale(
            scale: 0.8333333333 + local.animationValue * 0.1666666667,
            filterQuality: FilterQuality.high,
            child: Center(
              child: Text(
                name,
                style: TextStyle(
                  fontSize: 16.8,
                  color: Color.lerp(colorScheme.onSurface, colorScheme.onSecondary, local.animationValue),
                ),
              ),
            ),
          );
        },
        onChanged: (state) => _setProxyState(state),
      ),
    );
  }
}
