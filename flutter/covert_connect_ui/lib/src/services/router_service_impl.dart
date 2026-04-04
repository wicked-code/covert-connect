import 'dart:convert';

import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/services/utils/serialization.dart';
import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

class RouterServiceImpl implements RouterServiceBase {
  static const kConfigKey = "proxy_config${kDebugMode ? "_debug" : ""}";
  static const kDefaultPort = 25445;

  static Future<RouterServiceBase> create() async {
    final router = RouterServiceImpl();
    await router.init();
    return router;
  }

  final router = ClientService();

  Future<void> init() async {
    final prefs = SharedPreferencesAsync();
    String configStr = await prefs.getString(kConfigKey) ?? "";
    ClientConfig cfg;
    if (configStr.isNotEmpty) {
      cfg = proxyConfigFromString(configStr);
    } else {
      cfg = ClientConfig(state: ClientState.off, directDomains: [], directApps: [], servers: []);
    }

    await router.start(cfg: cfg);
  }

  @override
  Future<ClientState> getState() => router.getState();

  @override
  Future<ClientStatus> getStatus() => router.getStatus();

  @override
  Future<void> setState(ClientState state) async {
    try {
      await router.setState(state: state);
    } catch (e) {
      log(e.toString());
      rethrow;
    }

    saveConfig();
  }

  @override
  Future<void> setServerEnabled(String host, bool value) async {
    try {
      await router.setServerEnabled(host: host, value: value);
    } catch (e) {
      log(e.toString());
      rethrow;
    }

    saveConfig();
  }

  @override
  Future<ProtocolConfig> getServerProtocol(String host, String key) async {
    return router.getServerProtocol(server: host, key: key);
  }

  @override
  Future<List<String>> getDomains() {
    return router.getDirectDomains();
  }

  @override
  Future<List<String>> getApps() {
    return router.getDirectApps();
  }

  @override
  Future<void> setDomain(String domain, String serverHost) async {
    await router.setDomain(domain: domain, serverHost: serverHost);
    await saveConfig();
  }

  @override
  Future<void> removeDomain(String domain) async {
    await router.removeDomain(domain: domain);
    saveConfig();
  }

  @override
  Future<void> setApp(String app, String serverHost) async {
    await router.setApp(app: app, serverHost: serverHost);
    await saveConfig();
  }

  @override
  Future<void> removeApp(String app) async {
    await router.removeApp(app: app);
    saveConfig();
  }

  @override
  Future<bool> checkDomain(String domain) {
    return ClientService.checkDomain(domain: domain);
  }

  @override
  Future<void> addServer(ServerConfig config) async {
    await router.addServer(config: config);
    saveConfig();
  }

  @override
  Future<void> updateServer(String origHost, ServerConfig newConfig) async {
    await router.updateServer(origHost: origHost, newConfig: newConfig);
    saveConfig();
  }

  @override
  Future<void> deleteServer(String host) async {
    await router.deleteServer(host: host);
    saveConfig();
  }

  @override
  Future<int> getTTFB(String server, String domain) async {
    return router.getTtfb(server: server, domain: domain);
  }

  @override
  Future<bool> getAutostart() {
    return ClientService.getAutostart();
  }

  @override
  Future<void> setAutostart(bool enabled) {
    return ClientService.setAutostart(enabled: enabled);
  }

  @override
  Future<void> log(String message, {LogErrorType? type}) async {
    if (type != null) {
      switch (type) {
        case LogErrorType.message:
          break;
        case LogErrorType.warning:
          message = "\x1B[33m$message";
          break;
        case LogErrorType.error:
          message = "\x1B[31m$message";
          break;
      }
    }
    ClientService.log(message: message);
  }

  @override
  Future<BigInt> registerLogger(Future<void> Function(String) callback) {
    return router.registerLogger(callback: callback);
  }

  @override
  Future<void> unregisterLogger(BigInt id) async {
    return router.unregisterLogger(id: id);
  }

  @override
  Future<List<LogLine>> getLog(BigInt? start, int limit) =>
      ClientService.getLog(start: start, limit: BigInt.from(limit));

  Future<void> saveConfig() async {
    final cfg = await router.getConfig();
    String json = jsonEncode(
      cfg,
      toEncodable: (Object? value) => value is ClientConfig
          ? proxyCofigToJson(value)
          : throw UnsupportedError('Saving proxy config: Cannot convert to JSON: $value'),
    );
    final prefs = SharedPreferencesAsync();
    prefs.setString(kConfigKey, json);
  }
}
