import 'dart:async';
import 'dart:convert';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/widgets/log_message.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/services/retainable_scroll.dart';
import 'package:flutter/material.dart';

const kReadChunkSize = 100;

class LogPage extends StatefulWidget {
  const LogPage({super.key});

  @override
  State<LogPage> createState() => _LogPageState();
}

class _LogPageState extends State<LogPage> {
  final List<LogLine> _logMessages = [];
  bool _endReached = false;

  BigInt? _loggerId;
  final List<String> _beforeInitMessages = [];

  void _loadMore() async {
    if (_endReached) return;

    final lastPosition = _logMessages.last.position;
    final newMessages = await di<ProxyServiceBase>().getLog(lastPosition, kReadChunkSize);
    if (newMessages.isEmpty) {
      _endReached = true;
      return;
    }

    _logMessages.addAll(newMessages);
    _updateIfMounted();
  }

  Future<void> _onLogMessage(String message) async {
    if (_logMessages.isEmpty) {
      _beforeInitMessages.add(message);
      return;
    }

    _logMessages.insert(0, LogLine(position: BigInt.zero, line: message));
    _updateIfMounted();
  }

  void _disposeLogger() {
    if (_loggerId != null) {
      di<ProxyServiceBase>().unregisterLogger(_loggerId!);
      _loggerId = null;
    }
  }

  void _init() async {
    _loggerId = await di<ProxyServiceBase>().registerLogger(_onLogMessage);

    final newMessages = (await di<ProxyServiceBase>().getLog(null, kReadChunkSize)).reversed.toList();

    // Remove trailing empty lines
    int pos = newMessages.length;
    while(pos > 0) {
      if (newMessages[pos - 1].line.isNotEmpty) {
        break;
      }
      pos --;
    }
    newMessages.removeRange(pos, newMessages.length);
    _logMessages.addAll(newMessages);

    for (final msg in _beforeInitMessages) {
      if (_logMessages.indexWhere((x) => x.line == msg) == -1) {
        _logMessages.insert(0, LogLine(position: BigInt.zero, line: msg));
      }
    }
    _beforeInitMessages.clear();

    _updateIfMounted();
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    _init();
    super.initState();
  }

  @override
  void dispose() {
    _disposeLogger();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(8.0),
        child: CustomScrollView(
          reverse: true,
          shrinkWrap: true,
          slivers: <Widget>[
            SliverList(
              delegate: SliverChildBuilderDelegate((BuildContext context, int index) {
                if (index == _logMessages.length - 1) {
                  _loadMore();
                }

                return LogMessage(message: LogMessageDto.fromJson(jsonDecode(_logMessages[index].line)));
              }, childCount: _logMessages.length),
            ),
          ],
        ),
      ),
    );
  }
}
