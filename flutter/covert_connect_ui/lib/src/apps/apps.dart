import 'dart:io';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/apps/add_app.dart';
import 'package:covert_connect/src/apps/processes.dart';
import 'package:covert_connect/src/utils/router.dart';
import 'package:covert_connect/src/widgets/route_list.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:covert_connect/src/widgets/toast.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

class AppsPage extends StatefulWidget {
  const AppsPage({super.key});

  @override
  State<AppsPage> createState() => _AppsPageState();
}

class _AppsPageState extends State<AppsPage> {
  List<RouteInfo> _apps = [];
  List<RouteInfo> _filtered = [];

  final TextEditingController _inputController = TextEditingController();
  String get _inputValue => _inputController.text;

  Future<void> _loadApps() async {
    final [rootApps as List<String>, status as ClientStatus] = await Future.wait([
      di<RouterServiceBase>().getApps(),
      di<RouterServiceBase>().getStatus(),
    ]);
    _apps = rootApps.map((d) => RouteInfo(d, "")).toList();
    for (final srv in status.servers) {
      _apps.addAll(srv.config.apps?.map((d) => RouteInfo(d, srv.config.host)) ?? []);
    }

    _filterApps(_inputValue);
    _updateIfMounted();
  }

  void _addApp() async {
    if (_inputValue.isEmpty) return;

    final status = await di<RouterServiceBase>().getStatus();
    if (!mounted) return;

    final result = await AddAppDialog.show(context, app: _inputValue, servers: status.servers);

    if (result == true) {
      _inputController.clear();
      await _loadApps();
    }
  }

  void _editApp(RouteInfo info) async {
    final status = await di<RouterServiceBase>().getStatus();
    if (!mounted) return;

    final result = await AddAppDialog.show(
      context,
      app: info.value,
      servers: status.servers,
      selectedServer: info.server,
    );
    if (result == true) {
      await _loadApps();
    }
  }

  void _deleteApp(RouteInfo info) async {
    await di<RouterServiceBase>().removeApp(info.value);
    await _loadApps();
    if (!mounted) return;

    Toast.warning(
      context,
      caption: info.value,
      text: "was removed, ${isDesktop ? 'click to undo' : 'tap to undo'}",
      onTap: () async {
        await di<RouterServiceBase>().setApp(info.value, info.server);
        await _loadApps();
      },
    );
  }

  void _inputChanged(String inputValue) {
    String withoutPath = inputValue.trim().split("/").last.split(r"\").last;
    if (withoutPath != inputValue) {
      _inputController.text = withoutPath;
    }

    _filterApps(withoutPath);
    _updateIfMounted();
  }

  void _filterApps(String value) {
    if (value.isNotEmpty) {
      _filtered = _apps.where((d) => d.value.contains(value)).toList();
    } else {
      _filtered = _apps;
    }
  }

  void _selectExecutable() async {
    XTypeGroup executables = XTypeGroup(
      label: 'Applications',
      extensions: Platform.isWindows ? <String>['exe'] : null,
      uniformTypeIdentifiers: <String>['public.executable', 'public.unix-executable'],
    );
    final XFile? file = await openFile(acceptedTypeGroups: <XTypeGroup>[executables]);
    if (file != null) {
      _inputChanged(file.path);
      _addApp();
    }
  }

  void _selectProcess() async {
    final process = await context.cupertinoGoTo<String?>(ProcessPage());
    if (process != null) {
      _inputChanged(process);
      _addApp();
    }
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _inputController.dispose();
    super.dispose();
  }

  @override
  void initState() {
    _loadApps();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final inputIsNotEmpty = _inputValue.isNotEmpty;
    return Padding(
      padding: EdgeInsets.all(8),
      child: Column(
        spacing: 8,
        children: [
          Row(
            spacing: 2,
            children: [
              Expanded(
                child: Padding(
                  padding: EdgeInsets.symmetric(vertical: 4),
                  child: Input(
                    hint: "Enter app name...",
                    keyboardType: TextInputType.url,
                    controller: _inputController,
                    onChanged: _inputChanged,
                    icon: _inputValue.isNotEmpty
                        ? SizedBox(
                            height: 20,
                            width: 20,
                            child: AppIconButton(
                              asset: "assets/icons/delete.svg",
                              assetColor: Colors.redAccent,
                              onPressed: () => setState(() {
                                _inputController.clear();
                                _filterApps("");
                              }),
                            ),
                          )
                        : null,
                  ),
                ),
              ),
              SizedBox(width: 2),
              AppIconButton(asset: "assets/icons/plus.svg", onPressed: inputIsNotEmpty ? _addApp : null),
              AppIconButton(asset: "assets/icons/open-file.svg", onPressed: _selectExecutable),
              AppIconButton(asset: "assets/icons/bullet-list.svg", onPressed: _selectProcess),
            ],
          ),
          Flexible(
            child: RouteList(
              routeName: "Application",
              routes: _filtered,
              onDeleteRoute: _deleteApp,
              onEditRoute: _editApp,
            ),
          ),
        ],
      ),
    );
  }
}
