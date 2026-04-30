import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/log.dart';
import 'package:covert_connect/src/options/widgets/option_switch.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/utils/router.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';

class OptionsPage extends StatefulWidget {
  const OptionsPage({super.key});

  @override
  State<OptionsPage> createState() => _OptionsPageState();
}

class _OptionsPageState extends State<OptionsPage> {
  final TextEditingController _controller = TextEditingController();
  bool _autostart = false;

  String _version = "";
  String _build = "";

  void _initVersion() async {
    PackageInfo packageInfo = await PackageInfo.fromPlatform();
    _version = packageInfo.version;
    _build = packageInfo.buildNumber;
    _updateIfMounted();
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
    _initVersion();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final textTheme = theme.textTheme;

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
                // TODO: ??? add more options here
                // Row(
                //   children: [
                //     Expanded(child: Text("Start on boot", style: TextStyle(height: 1.0))),
                //     OptionSwitch(
                //       value: _autostart,
                //       onToggle: setAutostart,
                //     ),
                //   ],
                // ),
                SizedBox(height: 6),
                Text("The app will be launched after reboot", style: grayedTextStyle),
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
