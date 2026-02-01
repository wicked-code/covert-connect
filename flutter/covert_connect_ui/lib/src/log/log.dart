import 'dart:async';
import 'dart:convert';
import 'dart:developer';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/widgets/log_message.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:flutter/material.dart';

const kReadChunkSize = 100;
const kLoadMoreThreshold = 25;

class LogPage extends StatefulWidget {
  const LogPage({super.key});

  @override
  State<LogPage> createState() => _LogPageState();
}

class _LogPageState extends State<LogPage> {
  final Key _centerKey = const ValueKey('bottom-sliver');

  final _scrollController = ScrollController();

  final List<String> _newMessages = [];
  final List<LogLine> _oldMessages = [];
  bool _endReached = false;

  BigInt? _loggerId;

  bool _loadMoreInProgress = false;

  void _loadMore() async {
    if (_endReached || _loadMoreInProgress || _oldMessages.isEmpty) return;

    _loadMoreInProgress = true;
    try {
      final lastPosition = _oldMessages.last.position;
      final newMessages = await di<ProxyServiceBase>().getLog(lastPosition, kReadChunkSize);
      if (newMessages.length < kReadChunkSize) {
        _endReached = true;
        return;
      }

      _oldMessages.addAll(newMessages);
      _updateIfMounted();
    } finally {
      _loadMoreInProgress = false;
    }
  }

  Future<void> _onLogMessage(String message) async {
    _newMessages.add(message);
    _updateIfMounted();

    if (_scrollController.hasClients && _scrollController.offset > _scrollController.position.maxScrollExtent - 64) {
      Future.delayed(Durations.short1);
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: Durations.medium1,
          curve: Curves.easeOut,
        ),
      );
    }
  }

  void _disposeLogger() {
    if (_loggerId != null) {
      di<ProxyServiceBase>().unregisterLogger(_loggerId!);
      _loggerId = null;
    }
  }

  void _init() async {
    _loggerId = await di<ProxyServiceBase>().registerLogger(_onLogMessage);

    List<LogLine> messages = (await di<ProxyServiceBase>().getLog(null, kReadChunkSize)).toList();
    bool fullChunk = messages.length >= kReadChunkSize;

    // Remove trailing empty lines
    int pos = 0;
    while (pos < messages.length) {
      if (messages[pos].line.isNotEmpty) {
        break;
      }
      pos++;
    }
    messages.removeRange(0, pos);

    if (fullChunk) {
      int splitIndex = messages.length ~/ 2;
      _oldMessages.addAll(messages.sublist(splitIndex));
      messages = messages.sublist(0, splitIndex);
    }

    // Remove possible duplicats
    var newMessages = messages.reversed.map((m) => m.line);
    if (_newMessages.isNotEmpty) {
      newMessages = newMessages.where((m) => !_newMessages.contains(m));
    }
    _newMessages.insertAll(0, newMessages);

    _updateIfMounted();

    if (_scrollController.hasClients) {
      Future.delayed(Durations.short1);
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollController.animateTo(
          _scrollController.position.maxScrollExtent + 100,
          duration: Durations.medium1,
          curve: Curves.easeOut,
        ),
      );
    }
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

  Widget buildLogMessage(String messageStr) {
    try {
      final message = LogMessageDto.fromJson(jsonDecode(messageStr));
      return LogMessage(message: message);
    } catch (e) {
      log("Error create message: $e");
    }
    return Container();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(8.0),
        child: CustomScrollView(
          center: _centerKey,
          controller: _scrollController,
          slivers: <Widget>[
            SliverList(
              delegate: SliverChildBuilderDelegate((BuildContext context, int index) {
                if (index < kLoadMoreThreshold) {
                  _loadMore();
                }
                return buildLogMessage(_oldMessages[index].line);
              }, childCount: _oldMessages.length),
            ),
            SliverList(
              key: _centerKey,
              delegate: SliverChildBuilderDelegate((BuildContext context, int index) {
                return buildLogMessage(_newMessages[index]);
              }, childCount: _newMessages.length),
            ),
          ],
        ),
      ),
    );
  }
}
