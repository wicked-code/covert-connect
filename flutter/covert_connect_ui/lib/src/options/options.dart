import 'package:collection/collection.dart';
import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/log.dart';
import 'package:covert_connect/src/options/widgets/option_switch.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/utils/router.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/button.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:flutter/material.dart';
import 'package:animated_toggle_switch/animated_toggle_switch.dart';
import 'package:package_info_plus/package_info_plus.dart';

class OptionsPage extends StatefulWidget {
  const OptionsPage({super.key});

  @override
  State<OptionsPage> createState() => _OptionsPageState();
}

class _OptionsPageState extends State<OptionsPage> {
  final TextEditingController _controller = TextEditingController();
  int _proxyPort = 0;
  bool _autostart = false;
  RouterMode? _mode;

  String _version = "";
  String _build = "";

  RouterMode _toRouterMode(String value) {
    return RouterMode.values.firstWhereOrNull((x) => x.name == value) ?? RouterMode.proxy;
  }

  void _setRouterMode(RouterMode mode) async {
    await di<RouterServiceBase>().setMode(mode);
    await _initMode();
  }

  void _initPort() async {
    _proxyPort = await di<RouterServiceBase>().getProxyPort();
    _controller.text = _proxyPort.toString();
    _updateIfMounted();
  }

  Future<void> _initAutoStart() async {
    _autostart = await di<RouterServiceBase>().getAutostart();
    _updateIfMounted();
  }

  void _initVersion() async {
    PackageInfo packageInfo = await PackageInfo.fromPlatform();
    _version = packageInfo.version;
    _build = packageInfo.buildNumber;
    _updateIfMounted();
  }

  Future<void> _initMode() async {
    _mode = await di<RouterServiceBase>().getMode();
    _updateIfMounted();
  }

  void setAutostart(bool value) async {
    await di<RouterServiceBase>().setAutostart(value);
    await _initAutoStart();
  }

  bool _isPortChangedAndValid() {
    final value = int.tryParse(_controller.text);
    return (value != null) && (value != _proxyPort);
  }

  void _applyPort() async {
    final value = int.tryParse(_controller.text);
    if (value == null) return;

    await di<RouterServiceBase>().setProxyPort(value);
    setState(() {
      _proxyPort = value;
    });
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  void initState() {
    _initMode();
    _initPort();
    _initAutoStart();
    _initVersion();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final textTheme = theme.textTheme;
    final colorScheme = theme.colorScheme;

    final grayedColor = textTheme.bodySmall?.color?.withValues(alpha: 0.5);
    final grayedTextStyle = textTheme.bodySmall?.copyWith(color: grayedColor);
    return Scaffold(
      body: Padding(
        padding: EdgeInsetsGeometry.symmetric(horizontal: 12, vertical: 12),
        child: Column(
          children: [
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(child: Text("Start on boot", style: TextStyle(height: 1.0))),
                    OptionSwitch(
                      value: _autostart,
                      onToggle: setAutostart,
                    ),
                  ],
                ),
                SizedBox(height: 6),
                Text("The app will be launched after reboot", style: grayedTextStyle),
                Container(margin: EdgeInsets.only(top: 8, bottom: 12), color: theme.dividerColor, height: 1),
                Row(
                  children: [
                    Expanded(child: Text("Mode", style: TextStyle(height: 1.0))),
                    AnimatedToggleSwitch<String>.size(
                      current: _mode?.name ?? "",
                      values: [RouterMode.proxy.name, RouterMode.tun.name],
                      borderWidth: 0,
                      spacing: 2,
                      iconOpacity: 0.67,
                      selectedIconScale: 1.0,
                      height: 25,
                      indicatorSize: const Size(56.0, 25.0),
                      loading: false,
                      iconAnimationType: AnimationType.onHover,
                      styleAnimationType: AnimationType.onHover,
                      style: ToggleStyle(borderColor: Colors.transparent, borderRadius: BorderRadius.circular(8)),
                      allowUnlistedValues: true,
                      customIconBuilder: (context, local, global) {
                        final name = switch (_toRouterMode(local.value)) {
                          RouterMode.proxy => 'Proxy',
                          RouterMode.tun => 'Tun',
                        };
                        return Transform.scale(
                          scale: 0.91 + local.animationValue * 0.17,
                          filterQuality: FilterQuality.high,
                          child: Center(
                            child: Text(
                              name,
                              style: TextStyle(
                                fontSize: 12,
                                color: Color.lerp(
                                  colorScheme.onSurface.withValues(alpha: 0.5),
                                  colorScheme.onSurface,
                                  local.animationValue,
                                ),
                              ),
                            ),
                          ),
                        );
                      },
                      onChanged: (mode) => _setRouterMode(_toRouterMode(mode)),
                    ),
                  ],
                ),
                SizedBox(height: 8),
                Text("Proxy mode: http only (not support by some apps)", style: grayedTextStyle),
                Text("Tun mode: tcp/udp traffic (all apps)", style: grayedTextStyle),
                Container(margin: EdgeInsets.symmetric(vertical: 8), color: theme.dividerColor, height: 1),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Expanded(
                      child: Column(
                        spacing: 3,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text("Proxy port"),
                          SizedBox(
                            width: 71,
                            child: Input(
                              controller: _controller,
                              textAlign: TextAlign.center,
                              onChanged: (_) => setState(() {}),
                              keyboardType: TextInputType.number,
                              padding: EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                            ),
                          ),
                        ],
                      ),
                    ),
                    Button(
                      label: "Apply",
                      padding: EdgeInsets.symmetric(horizontal: 12, vertical: 3),
                      onTap: _isPortChangedAndValid() ? _applyPort : null,
                    ),
                  ],
                ),
              ],
            ),
            Expanded(child: Container()),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text("Covert-Connect $_version build $_build", style: grayedTextStyle),
                AppIconButton(
                  asset: "assets/icons/log.svg",
                  assetColor: grayedColor,
                  onPressed: () => context.slideGoTo(LogPage()),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
