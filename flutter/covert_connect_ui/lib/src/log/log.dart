import 'dart:async';
import 'dart:convert';
import 'dart:developer';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/log/widgets/log_message.dart';
import 'package:covert_connect/src/log/utils/log_message.dart';
import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:flutter/material.dart';

const kReadChunkSize = 100;
const kMaxUpdateChunkSize = kReadChunkSize * 5;
const kLogUpdateInterval = Duration(milliseconds: 500);
const kLoadMoreThreshold = 25;

class LogPage extends StatefulWidget {
  const LogPage({super.key});

  @override
  State<LogPage> createState() => _LogPageState();
}

class _LogPageState extends State<LogPage> {
  final Key _centerKey = const ValueKey('bottom-sliver');

  final _scrollController = ScrollController();

  final List<LogLine> _newMessages = [];
  final List<LogLine> _oldMessages = [];
  bool _endReached = false;

  bool _loadMoreInProgress = false;
  bool _updateInProgress = false;

  Timer? _updateTimer;

  void _loadMore() async {
    if (_endReached || _loadMoreInProgress || _oldMessages.isEmpty) return;

    _loadMoreInProgress = true;
    try {
      final lastPosition = _oldMessages.last.position;
      final newMessages = await di<RouterServiceBase>().getLog(lastPosition, null, kReadChunkSize);
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

  void _init() async {
    List<LogLine> messages = (await di<RouterServiceBase>().getLog(null, null, kReadChunkSize)).toList();
    bool fullChunk = messages.length >= kReadChunkSize;
    if (!mounted) return;

    if (fullChunk) {
      int splitIndex = messages.length ~/ 2;
      _oldMessages.addAll(messages.sublist(splitIndex));
      messages = messages.sublist(0, splitIndex);
    }
    _newMessages.insertAll(0, messages.reversed);      

    _updateTimer ??= Timer.periodic(kLogUpdateInterval, (timer) {
      _updateLog();
    });

    _updateIfMounted();

    if (_scrollController.hasClients) {
      await Future.delayed(Durations.short1);
      if (!mounted) return;
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollController.animateTo(
          _scrollController.position.maxScrollExtent + 100,
          duration: Durations.medium1,
          curve: Curves.easeOut,
        ),
      );
    }
  }

  void _updateLog() async {
    if (_updateInProgress || _loadMoreInProgress) return;

    _updateInProgress = true;
    try {
      final lastPosition = _newMessages.lastOrNull?.position;
      final newMessages = await di<RouterServiceBase>().getLog(null, lastPosition, kMaxUpdateChunkSize);
      if (newMessages.length >= kMaxUpdateChunkSize) {
        // too many new messages, reset log view
        _oldMessages.clear();
        _newMessages.clear();
        _endReached = false;
        _updateTimer?.cancel();
        _updateTimer = null;
        _init();
        return;
      }

      _newMessages.addAll(newMessages.reversed);
    } finally {
      _updateInProgress = false;
    }

    _updateIfMounted();
    if (_scrollController.hasClients && _scrollController.offset > _scrollController.position.maxScrollExtent - 64) {
      await Future.delayed(Durations.short1);
      if (!mounted) return;
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
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
    _updateTimer?.cancel();
    _scrollController.dispose();
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
                return buildLogMessage(_newMessages[index].line);
              }, childCount: _newMessages.length),
            ),
          ],
        ),
      ),
    );
  }
}
