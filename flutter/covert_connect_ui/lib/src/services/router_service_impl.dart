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

  final router = RouterService();

  Future<void> init() async {
    final prefs = SharedPreferencesAsync();
    String configStr = await prefs.getString(kConfigKey) ?? "";
    RouterConfig cfg;
    if (configStr.isNotEmpty) {
      cfg = proxyConfigFromString(configStr);
    } else {
      cfg = RouterConfig(state: RouterState.off, mode: RouterMode.proxy, proxyPort: kDefaultPort, domains: [], apps: [], servers: []);
    }

    await router.start(cfg: cfg);
  }

  @override
  Future<RouterState> getState() => router.getState();

  @override
  Future<RouterStatus> getStatus() => router.getStatus();

  @override
  Future<void> setState(RouterState state) async {
    try {
      await router.setState(state: state);
    } catch (e) {
      log(e.toString());
      rethrow;
    }

    saveConfig();
  }

  @override
  Future<RouterMode> getMode() => router.getMode();

  @override
  Future<void> setMode(RouterMode mode) async {
    try {
      await router.setMode(mode: mode);
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
    return router.getDomains();
  }

  @override
  Future<List<String>> getApps() {
    return router.getApps();
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
    return RouterService.checkDomain(domain: domain);
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
  Future<int> getProxyPort() async {
    return await router.getProxyPort();
  }

  @override
  Future<void> setProxyPort(int port) async {
    await router.setProxyPort(port: port);
    saveConfig();
  }

  @override
  Future<bool> getAutostart() {
    return RouterService.getAutostart();
  }

  @override
  Future<void> setAutostart(bool enabled) {
    return RouterService.setAutostart(enabled: enabled);
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
    RouterService.log(message: message);
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
      RouterService.getLog(start: start, limit: BigInt.from(limit));

  Future<void> saveConfig() async {
    final cfg = await router.getConfig();
    String json = jsonEncode(
      cfg,
      toEncodable: (Object? value) => value is RouterConfig
          ? proxyCofigToJson(value)
          : throw UnsupportedError('Saving proxy config: Cannot convert to JSON: $value'),
    );
    final prefs = SharedPreferencesAsync();
    prefs.setString(kConfigKey, json);
  }
}
