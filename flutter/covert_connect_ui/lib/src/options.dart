import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/log.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/router.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/button.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:flutter/material.dart';
import 'package:flutter_switch/flutter_switch.dart';
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

  String _version = "";
  String _build = "";

  void _initPort() async {
    _proxyPort = await di<ProxyServiceBase>().getProxyPort();
    _controller.text = _proxyPort.toString();
    _updateIfMounted();
  }

  void _initAutoStart() async {
    _autostart = await di<ProxyServiceBase>().getAutostart();
    _updateIfMounted();
  }

  void _initVersion() async {
    PackageInfo packageInfo = await PackageInfo.fromPlatform();
    _version = packageInfo.version;
    _build = packageInfo.buildNumber;
    _updateIfMounted();
  }

  void setAutostart(bool value) async {
    await di<ProxyServiceBase>().setAutostart(value);
    _autostart = await di<ProxyServiceBase>().getAutostart();
    if (mounted) setState(() {});
  }

  bool _isPortChangedAndValid() {
    final value = int.tryParse(_controller.text);
    return (value != null) && (value != _proxyPort);
  }

  void _applyPort() async {
    final value = int.tryParse(_controller.text);
    if (value == null) return;

    await di<ProxyServiceBase>().setProxyPort(value);
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
        padding: EdgeInsetsGeometry.symmetric(horizontal: 12, vertical: 16),
        child: Column(
          children: [
            Column(
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Column(
                        spacing: 4,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text("Start on boot", style: TextStyle(height: 1.0)),
                          Text("The app will not be launched after reboot", style: grayedTextStyle),
                        ],
                      ),
                    ),
                    FlutterSwitch(
                      height: 25.0,
                      width: 48.0,
                      padding: 4.0,
                      toggleSize: 20.0,
                      borderRadius: 15.0,
                      activeColor: colorScheme.secondary,
                      activeToggleColor: colorScheme.onSecondary,
                      inactiveToggleColor: colorScheme.outline,
                      inactiveColor: darken(colorScheme.surface, 0.95, 1.05, theme.brightness).withValues(alpha: 0.57),
                      activeSwitchBorder: Border.all(color: colorScheme.secondary, width: 1),
                      inactiveSwitchBorder: Border.all(color: colorScheme.outline, width: 1),
                      value: _autostart,
                      onToggle: setAutostart,
                    ),
                  ],
                ),
                Container(margin: EdgeInsets.symmetric(vertical: 8), color: theme.dividerColor, height: 2),
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
