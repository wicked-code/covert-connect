import 'dart:async';
import 'dart:math';

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/status/widgets/server_list.dart';
import 'package:covert_connect/src/status/widgets/state_toggle.dart';
import 'package:covert_connect/src/status/widgets/traffic_graph.dart';
import 'package:covert_connect/src/widgets/toast.dart';
import 'package:flutter/material.dart';
import 'package:ntp/ntp.dart';

const kMaxSyncOffsetMs = 5 * 1000; // 5 seconds
const kCheckSyncInterval = Duration(minutes: 60);
final kUpdateIntervalMs = kUpdateInterval.inMilliseconds;
double timeFromIndex(int index) => (index * kUpdateIntervalMs).toDouble();

class StatusPage extends StatefulWidget {
  const StatusPage({super.key});

  @override
  State<StatusPage> createState() => _StatusPageState();
}

class _StatusPageState extends State<StatusPage> with AutomaticKeepAliveClientMixin {
  late Timer _timer;
  ProxyStateFull? _proxyStateFull;

  TrafficSample? _prevSample;
  final _speedHistory = List.generate(
    kTrafficGraphSamplesCount,
    (index) => TrafficSample(time: timeFromIndex(index), rx: 0, tx: 0),
    growable: true,
  );

  double _time = timeFromIndex(kTrafficGraphSamplesCount);

  bool _enabled = true;

  DateTime? _lastSyncCheck;

  Future<void> _updateServers() async {
    _proxyStateFull = await di<ProxyServiceBase>().getStateFull();
  }

  void _update() async {
    if (!di.isReadySync<ProxyServiceBase>()) return;
    _checkSync();

    await _updateServers();
    final newSample = _proxyStateFull!.servers.fold(
      TrafficSample(time: _time, rx: 0, tx: 0),
      (acc, server) => TrafficSample(
        time: _time,
        rx: acc.rx + server.state.rxTotal.toDouble(),
        tx: acc.tx + server.state.txTotal.toDouble(),
      ),
    );
    if (_prevSample != null) {
      if (_speedHistory.length >= kTrafficGraphSamplesCount) {
        _speedHistory.removeAt(0);
      }
      _speedHistory.add(
        TrafficSample(
          time: _time,
          rx: (newSample.rx - _prevSample!.rx) * 1000 / kUpdateIntervalMs,
          tx: (newSample.tx - _prevSample!.tx) * 1000 / kUpdateIntervalMs,
        ),
      );
    }
    _prevSample = newSample;
    _time += kUpdateIntervalMs;

    if (mounted && _enabled) setState(() {});
  }

  void _checkSync() async {
    if (!_enabled) return;
    if (_lastSyncCheck != null && DateTime.now().difference(_lastSyncCheck!) < kCheckSyncInterval) {
      return;
    }

    _lastSyncCheck = DateTime.now();
    DateTime startDate = DateTime.now().toLocal();
    int offset = await NTP.getNtpOffset(localTime: startDate);
    if (mounted && offset > kMaxSyncOffsetMs) {
      Toast.warning(
        context,
        caption: "Time is not synchronized",
        text: "Please synchronize your system time to avoid connection issues.",
        autoCloseDuration: Duration.zero,
      );
    }
  }

  void _onAppStateChange() {
    _enabled = di<AppStateService>().value == AppState.visible;
    setState(() {});
  }

  @override
  void initState() {
    di<AppStateService>().addListener(_onAppStateChange);
    _onAppStateChange();
    _update();
    _timer = Timer.periodic(kUpdateInterval, (_) => _update());
    super.initState();
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    final height = MediaQuery.of(context).size.height;

    if (_proxyStateFull == null || !_proxyStateFull!.initialized) {
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }

    final double graphHeight = min(200, max(100, height - 42 * _proxyStateFull!.servers.length - 200));
    return Scaffold(
      body: Column(
        children: [
          StateToggle(),
          Flexible(
            child: ServerList(servers: _proxyStateFull!.servers, updateServers: _updateServers),
          ),
          SizedBox(height: 24),
          SizedBox(
            height: graphHeight,
            child: TrafficGraph(data: _speedHistory, height: graphHeight),
          ),
          SizedBox(height: 24),
        ],
      ),
    );
  }

  @override
  bool get wantKeepAlive => true;
}
