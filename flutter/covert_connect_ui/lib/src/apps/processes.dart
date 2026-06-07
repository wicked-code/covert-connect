import 'dart:io';

import 'package:covert_connect/src/apps/widgets/app_list.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:flutter/material.dart';

class ProcessPage extends StatefulWidget {
  const ProcessPage({super.key});

  @override
  State<ProcessPage> createState() => _ProcessPageState();
}

class _ProcessPageState extends State<ProcessPage> {
  List<AppInfo> _apps = [];
  List<AppInfo> _filtered = [];

  final TextEditingController _inputController = TextEditingController();
  String get _inputValue => _inputController.text;

  Future<List<AppInfo>> getRunningProcesses() async {
    List<AppInfo> apps = [];

    if (Platform.isWindows) {
      final result = await Process.run('wmic', [
        'process',
        'where',
        'ExecutablePath is not null',
        'get',
        'ExecutablePath,ProcessId',
        '/format:csv',
      ]);
      apps = _parseWindowsCsv(result.stdout as String);
    } else {
      final result = await Process.run('ps', ['-eo', 'pid,args']);
      apps = _parsePosix(result.stdout as String);
    }

    final seenPaths = <String>{};
    return apps.where((app) => seenPaths.add(app.path)).toList();
  }

  List<AppInfo> _parseWindowsCsv(String output) {
    final List<AppInfo> results = [];
    final lines = output.split('\n');

    for (var line in lines) {
      final trimmed = line.trim();
      // WMIC CSV format usually returns: Node,ExecutablePath,ProcessId
      if (trimmed.isEmpty || trimmed.startsWith('Node')) continue;

      final parts = trimmed.split(',');
      if (parts.length >= 3) {
        final path = parts[1];
        final pid = int.tryParse(parts[2]);
        if (path.isNotEmpty && pid != null) {
          results.add(AppInfo(path: path, pid: pid));
        }
      }
    }
    return results;
  }

  List<AppInfo> _parsePosix(String output) {
    final List<AppInfo> results = [];
    final lines = output.trim().split('\n');

    // Skip header line
    for (var i = 1; i < lines.length; i++) {
      final line = lines[i].trim();
      if (line.isEmpty) continue;

      // Split by first space: [PID, Path+Args]
      final firstSpace = line.indexOf(' ');
      if (firstSpace != -1) {
        final pidStr = line.substring(0, firstSpace).trim();
        final path = line.substring(firstSpace).trim();
        final pid = int.tryParse(pidStr);

        if (pid != null && path.isNotEmpty) {
          results.add(AppInfo(path: path, pid: pid));
        }
      }
    }
    return results;
  }

  Future<void> _loadProcesses() async {
    _apps = await getRunningProcesses();
    _filterApps(_inputValue);
    _updateIfMounted();
  }

  void _inputChanged(String inputValue) {
    _filterApps(_inputValue);
    _updateIfMounted();
  }

  void _filterApps(String value) {
    if (value.isNotEmpty) {
      _filtered = _apps.where((d) => d.path.contains(value) || d.pid.toString().contains(value)).toList();
    } else {
      _filtered = _apps;
    }
  }

  void _selectApp(AppInfo info) {
    Navigator.of(context).pop(info.path);
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
    _loadProcesses();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Padding(
        padding: EdgeInsets.all(8),
        child: Column(
          spacing: 8,
          children: [
            Padding(
              padding: EdgeInsets.symmetric(vertical: 4),
              child: Input(
                hint: "Type a name or PID to filter processes",
                keyboardType: TextInputType.text,
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
            Flexible(
              child: AppList(apps: _filtered, onSelect: _selectApp),
            ),
          ],
        ),
      ),
    );
  }
}
