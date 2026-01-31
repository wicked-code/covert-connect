import 'dart:async';
import 'dart:math';

import 'package:collection/collection.dart';
import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';
import 'package:covert_connect/src/services/proxy_service.dart';

class ProxyServiceMock implements ProxyServiceBase {
  static Future<ProxyServiceBase> create() async {
    return ProxyServiceMock();
  }

  @override
  Future<ProxyStateFull> getStateFull() async {
    if (_noValueCount <= 0) {
      servers = servers
          .map(
            (s) => s.copyWith(
              state: ServerState(
                rxTotal: s.state.rxTotal + BigInt.from(Random().nextInt(1000000)),
                txTotal: s.state.txTotal + BigInt.from(Random().nextInt(1000000)),
                errCount: s.state.errCount,
                succesCount: s.state.succesCount,
              ),
            ),
          )
          .toList();

      final tryNoValue = Random().nextInt(3000);
      if (tryNoValue < 50) {
        _noValueCount = tryNoValue;
      }
    } else {
      servers = servers
          .map(
            (s) => s.copyWith(
              state: ServerState(
                rxTotal: s.state.rxTotal,
                txTotal: s.state.txTotal,
                errCount: s.state.errCount,
                succesCount: s.state.succesCount,
              ),
            ),
          )
          .toList();

      _noValueCount--;
    }

    return ProxyStateFull(initialized: true, servers: servers);
  }

  @override
  Future<void> setServerEnabled(String host, bool value) async {
    final idx = servers.indexWhere((element) => element.config.host == host);
    if (idx == -1) throw "Server not found";

    servers[idx] = servers[idx].copyWith(config: servers[idx].config.copyWith(enabled: value));
  }

  @override
  Future<ProtocolConfig> getServerProtocol(String host, String key) async {
    var server = servers.firstWhereOrNull((srv) => srv.config.host == host);
    if (server == null && newServer.config.host == host) {
      server = newServer;
    }

    if (server == null) {
      throw "Invalid host";
    }

    if (server.config.protocol.key != key) {
      throw "Invalid key";
    }

    await Future.delayed(Duration(milliseconds: 3000));
    return server.config.protocol;
  }

  @override
  Future<ProxyState> getProxyState() async => _proxyState;

  @override
  Future<void> setProxyState(ProxyState state) async {
    _proxyState = state;
  }

  @override
  Future<List<String>> getDomains() async {
    return domains;
  }

  @override
  Future<List<String>> getApps() async {
    return apps;
  }

  @override
  Future<void> setDomain(String domain, String serverHost) async {
    if (serverHost.isNotEmpty) {
      domains.remove(domain);
      servers.forEachIndexed((index, srv) {
        if (srv.config.host == serverHost) {
          if (srv.config.domains == null) {
            servers.setAll(index, [srv.copyWith(config: srv.config.copyWith(domains: []))]);
          }
          if (!(srv.config.domains?.contains(domain) ?? false)) {
            srv.config.domains?.add(domain);
          }
        } else {
          srv.config.domains?.remove(domain);
        }
      });
    } else {
      if (!domains.contains(domain)) {
        domains.add(domain);
      }

      for (final srv in servers) {
        srv.config.domains?.remove(domain);
      }
    }
  }

  @override
  Future<void> removeDomain(String domain) async {
    domains.remove(domain);
    for (final srv in servers) {
      srv.config.domains?.remove(domain);
    }
  }

  @override
  Future<void> setApp(String app, String serverHost) async {
    if (serverHost.isNotEmpty) {
      apps.remove(app);
      servers.forEachIndexed((index, srv) {
        if (srv.config.host == serverHost) {
          if (srv.config.apps == null) {
            servers.setAll(index, [srv.copyWith(config: srv.config.copyWith(apps: []))]);
          }
          if (!(srv.config.apps?.contains(app) ?? false)) {
            srv.config.apps?.add(app);
          }
        } else {
          srv.config.apps?.remove(app);
        }
      });
    } else {
      if (!apps.contains(app)) {
        apps.add(app);
      }

      for (final srv in servers) {
        srv.config.apps?.remove(app);
      }
    }
  }

  @override
  Future<void> removeApp(String app) async {
    apps.remove(app);
    for (final srv in servers) {
      srv.config.apps?.remove(app);
    }
  }

  @override
  Future<bool> checkDomain(String domain) async {
    return true;
  }

  @override
  Future<void> addServer(ServerConfig config) async {
    if (servers.any((server) => server.config.host == config.host)) {
      throw Exception('Server with host ${config.host} already exists');
    }

    final hostPort = config.host.split(":");

    servers.add(
      ServerInfo(
        state: ServerState(rxTotal: BigInt.zero, txTotal: BigInt.zero, errCount: BigInt.zero, succesCount: BigInt.zero),
        config: config,
        ip: orginalServers.firstWhereOrNull((x) => x.config.host == config.host)?.ip ?? hostPort.first,
        port: hostPort.length > 1 ? int.parse(hostPort[1]) : 443,
      ),
    );
  }

  @override
  Future<void> updateServer(String origHost, ServerConfig newConfig) async {
    servers = servers
        .map(
          (server) => server.config.host == origHost
              ? ServerInfo(state: server.state, config: newConfig, ip: server.ip, port: server.port)
              : server,
        )
        .toList();
  }

  @override
  Future<void> deleteServer(String host) async {
    servers.removeWhere((server) => server.config.host == host);
  }

  @override
  Future<int> getTTFB(String server, String domain) async {
    final ping = Random().nextInt(1200);
    await Future.delayed(Duration(milliseconds: ping));
    return ping;
  }

  @override
  Future<void> log(String message, {LogErrorType? type}) async {
    logInternal(message, type: type);
  }

  String logInternal(String message, {LogErrorType? type}) {
    String level = "INFO";
    if (type != null) {
      switch (type) {
        case LogErrorType.message:
          break;
        case LogErrorType.warning:
          level = "WARN";
          break;
        case LogErrorType.error:
          level = "ERROR";
          break;
      }
    }
    final now = DateTime.now().toUtc().toIso8601String();
    final logMessage = "{\"timestamp\":\"$now\",\"level\":\"$level\",\"fields\":{\"message\":\"$message\"},\"target\":\"client::proxy\"}";
    _log.add(logMessage);
    return logMessage;
  }

  Timer? _timer;
  Future<void> Function(String)? _callback;

  void _sendLog() {
    final count = Random().nextInt(10) - 5;
    for(int i = 0; i < count; i++) {
      final chance = Random().nextInt(14);
      String value = "";
      if (chance < 6) {
        value = logInternal(r'proxy request from process: C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe - 127.0.0.1:62772');
      } else if (chance == 6 || chance == 7) {
        value = logInternal(r'server io error: peer closed connection without sending TLS close_notify: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof', type: LogErrorType.warning);
      } else if (chance == 8) {
        value = logInternal("some error with text", type: LogErrorType.error);
      } else if (chance == 9) {
        value = logInternal(r"some error, logn error, very very log erorr with path C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", type: LogErrorType.error);
      }

      if (value.isNotEmpty) {
        _callback?.call(_log.last);
      }
    }
  }

  @override
  Future<List<LogLine>> getLog(BigInt? start, int limit) async {
    if (start == BigInt.from(0)) return [];

    final pos = max(0, (start?.toInt() ?? _log.length) - limit);
    final logLines = _log.sublist(pos, min(_log.length, pos + limit));
    return logLines.mapIndexed((idx, line) => LogLine(line: line, position: BigInt.from(pos + idx))).toList().reversed.toList();
  }

  @override
  Future<BigInt> registerLogger(Future<void> Function(String) callback) async {
    _callback = callback;
    _timer ??= Timer.periodic(Duration(seconds: 1), (timer) => _sendLog());
    return BigInt.from(1);
  }

  @override
  Future<void> unregisterLogger(BigInt id) async {
    _timer?.cancel();
    _timer = null;
    _callback = null;
  }

  @override
  Future<int> getProxyPort() async {
    return _proxyPort;
  }

  @override
  Future<void> setProxyPort(int port) async {
    _proxyPort = port;
  }

  @override
  Future<bool> getAutostart() async {
    return _autostart;
  }

  @override
  Future<void> setAutostart(bool enabled) async {
    _autostart = enabled;
  }
}

extension ServerInfoEx on ServerInfo {
  ServerInfo copyWith({ServerState? state, ServerConfig? config, String? ip, int? port}) =>
      ServerInfo(state: state ?? this.state, config: config ?? this.config, ip: ip ?? this.ip, port: port ?? this.port);
}

extension ServerConfigEx on ServerConfig {
  ServerConfig copyWith({
    String? caption,
    String? host,
    int? weight,
    List<String>? domains,
    List<String>? apps,
    bool? enabled,
    ProtocolConfig? protocol,
  }) => ServerConfig(
    caption: caption ?? this.caption,
    host: host ?? this.host,
    weight: weight ?? this.weight,
    domains: domains ?? this.domains,
    apps: apps ?? this.apps,
    enabled: enabled ?? this.enabled,
    protocol: protocol ?? this.protocol,
  );
}

int _proxyPort = 25445;
bool _autostart = true;
int _noValueCount = 0;
ProxyState _proxyState = ProxyState.all;
List<String> _log = [
  r'{"timestamp":"2026-01-30T23:41:38.682840Z","level":"INFO","fields":{"message":"proxy server started: 127.0.0.1:25445"},"target":"client::proxy"}',
  r'{"timestamp":"2026-01-30T23:41:38.957804Z","level":"INFO","fields":{"message":"proxy request from process: C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe - 127.0.0.1:64372"},"target":"client::proxy"}',
  r'{"timestamp":"2026-01-30T23:41:40.632655Z","level":"INFO","fields":{"message":"proxy request from process: C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe - 127.0.0.1:59235"},"target":"client::proxy"}',
  r'{"timestamp":"2026-01-30T23:41:42.110039Z","level":"INFO","fields":{"message":"proxy request from process: C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe - 127.0.0.1:62420"},"target":"client::proxy"}',
  r'{"timestamp":"2026-01-30T23:46:41.761705Z","level":"ERROR","fields":{"message":"server io error: peer closed connection without sending TLS close_notify: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof"},"target":"client::proxy"}',
  r'{"timestamp":"2026-01-30T23:46:41.761705Z","level":"WARN","fields":{"message":"server io error: peer closed connection without sending TLS close_notify: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof"},"target":"client::proxy"}',
];

List<String> domains = [
  'test.com',
  'c333.space',
  'xyz.space',
  'gtest-very-long-domain-name.com',
  'gtest.space',
  'onemore.it',
];

List<String> apps = [
  'test-app'
];

List<ServerInfo> servers = [
  ServerInfo(
    state: ServerState(
      rxTotal: BigInt.from(0),
      txTotal: BigInt.from(0),
      errCount: BigInt.from(0),
      succesCount: BigInt.from(0),
    ),
    config: ServerConfig(
      caption: "",
      host: "ttt-ccc-covert-connect.com:8383",
      domains: ["x1.com", "x2.com", "x3.net", "tmp.space"],
      enabled: false,
      protocol: ProtocolConfig(
        key: "key",
        kdf: Kdf.argon2,
        cipher: CipherType.chaCha20Poly1305,
        maxConnectDelay: 10000,
        headerPadding: HeaderPadding(start: 50, end: 777),
        dataPadding: DataPadding(max: 250, rate: 10),
        encryptionLimit: BigInt.parse("18446744073709551615"),
      ),
    ),
    ip: "14.55.141.189",
    port: 8383,
  ),
  ServerInfo(
    state: ServerState(
      rxTotal: BigInt.from(935000),
      txTotal: BigInt.from(325000),
      errCount: BigInt.from(0),
      succesCount: BigInt.from(3),
    ),
    config: ServerConfig(
      caption: "",
      host: "cconnect.space",
      domains: ["xxx1.com", "xxx2.com", "xxx3.net", "tmp2.space"],
      apps: ["test-space-app"],
      enabled: true,
      protocol: ProtocolConfig(
        key: "key",
        kdf: Kdf.blake3,
        cipher: CipherType.aes256Gcm,
        maxConnectDelay: 10000,
        headerPadding: HeaderPadding(start: 50, end: 777),
        dataPadding: DataPadding(max: 250, rate: 10),
        encryptionLimit: BigInt.parse("18446744073709551615"),
      ),
    ),
    ip: "3.155.36.44",
    port: 443,
  ),
  ServerInfo(
    state: ServerState(
      rxTotal: BigInt.from(15319000),
      txTotal: BigInt.from(2325000),
      errCount: BigInt.from(1),
      succesCount: BigInt.from(23),
    ),
    config: ServerConfig(
      host: "5.255.96.144:8383",
      domains: [],
      enabled: true,
      protocol: ProtocolConfig(
        key: "Test-key-that-contains-forty-three-symbols!",
        kdf: Kdf.blake3,
        cipher: CipherType.aes256Gcm,
        maxConnectDelay: 10000,
        headerPadding: HeaderPadding(start: 50, end: 777),
        dataPadding: DataPadding(max: 250, rate: 10),
        encryptionLimit: BigInt.parse("18446744073709551615"),
      ),
    ),
    ip: "5.255.96.144",
    port: 8383,
  ),
];

List<ServerInfo> orginalServers = [...servers, newServer];

ServerInfo newServer = ServerInfo(
  state: ServerState(
    rxTotal: BigInt.from(15319000),
    txTotal: BigInt.from(2325000),
    errCount: BigInt.one,
    succesCount: BigInt.from(23),
  ),
  config: ServerConfig(
    caption: "Host1",
    host: "covert-connect.xyz:8383",
    domains: [],
    enabled: true,
    protocol: ProtocolConfig(
      key: "Test-key-that-contains-forty-three-symbols!",
      kdf: Kdf.blake3,
      cipher: CipherType.aes256Gcm,
      maxConnectDelay: 10000,
      headerPadding: HeaderPadding(start: 50, end: 777),
      dataPadding: DataPadding(max: 250, rate: 10),
      encryptionLimit: BigInt.parse("18446744073709551615"),
    ),
  ),
  ip: "2.143.89.114",
  port: 8383,
);
