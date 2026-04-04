import 'package:animated_toggle_switch/animated_toggle_switch.dart';
import 'package:collection/collection.dart';
import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:flutter/material.dart';

class StateToggle extends StatefulWidget {
  const StateToggle({super.key});

  @override
  State<StateToggle> createState() => _StateToggleState();
}

class _StateToggleState extends State<StateToggle> {
  ClientState? _state;

  void _setProxyState(ClientState state) async {
    await di<RouterServiceBase>().setState(state);
    _update();
  }

  void _update() async {
    _state = await di<RouterServiceBase>().getState();
    if (mounted) setState(() {});
  }

  ClientState _toProxyState(String value) {
    return ClientState.values.firstWhereOrNull((x) => x.name == value) ?? ClientState.off;
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
      child: AnimatedToggleSwitch<String>.size(
        active: _state != null,
        current: _state?.name ?? "",
        values: [ClientState.off.name, ClientState.smart.name, ClientState.all.name],
        borderWidth: 0,
        spacing: 2,
        iconOpacity: 0.67,
        selectedIconScale: 1.0,
        height: indicatorSize.height,
        indicatorSize: indicatorSize,
        loading: false,
        iconAnimationType: AnimationType.onHover,
        styleAnimationType: AnimationType.onHover,
        style: ToggleStyle(borderColor: Colors.transparent, borderRadius: borderRadius),
        allowUnlistedValues: true,
        indicatorAppearingDuration: Durations.short4,
        indicatorAppearingBuilder: (BuildContext context, double value, Widget indicator) {
          return Opacity(opacity: value.clamp(0.0, 1.0), child: indicator);
        },
        customSeparatorBuilder: (context, local, global) {
          final opacity = dividerColor.a * ((global.position - local.position).abs() - 0.5).clamp(0.0, 1.0);
          return VerticalDivider(indent: 10.0, endIndent: 10.0, color: dividerColor.withValues(alpha: opacity));
        },
        customIconBuilder: (context, local, global) {
          final name = switch (_toProxyState(local.value)) {
            ClientState.off => 'OFF',
            ClientState.smart => 'SMART',
            ClientState.all => 'ALL',
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
        onChanged: (state) => _setProxyState(_toProxyState(state)),
      ),
    );
  }
}
