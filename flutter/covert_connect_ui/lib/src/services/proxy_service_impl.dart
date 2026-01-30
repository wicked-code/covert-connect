import 'dart:convert';

import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/services/utils/serialization.dart';
import 'package:shared_preferences/shared_preferences.dart';

class ProxyServiceImpl implements ProxyServiceBase {
  static const kConfigKey = "proxy_config";
  static const kDefaultPort = 25445;

  static Future<ProxyServiceBase> create() async {
    final proxy = ProxyServiceImpl();
    await proxy.init();
    return proxy;
  }

  final proxy = ProxyService();

  Future<void> init() async {
    final prefs = SharedPreferencesAsync();
    String configStr = await prefs.getString(kConfigKey) ?? "";
    ProxyConfig cfg;
    if (configStr.isNotEmpty) {
      cfg = proxyConfigFromString(configStr);
    } else {
      cfg = ProxyConfig(state: ProxyState.off, port: kDefaultPort, domains: [], apps: [], servers: []);
    }

    await proxy.start(cfg: cfg);
  }

  @override
  Future<ProxyStateFull> getStateFull() => proxy.getState();

  @override
  Future<ProxyState> getProxyState() => proxy.getProxyState();

  @override
  Future<void> setProxyState(ProxyState state) async {
    try {
      await proxy.setProxyState(proxyState: state);
    } catch (e) {
      log(e.toString());
      rethrow;
    }

    saveConfig();
  }

  @override
  Future<void> setServerEnabled(String host, bool value) async {
    try {
      await proxy.setServerEnabled(host: host, value: value);
    } catch (e) {
      log(e.toString());
      rethrow;
    }

    saveConfig();
  }

  @override
  Future<ProtocolConfig> getServerProtocol(String host, String key) async {
    return proxy.getServerProtocol(server: host, key: key);
  }

  @override
  Future<List<String>> getDomains() {
    return proxy.getDomains();
  }

  @override
  Future<List<String>> getApps() {
    return proxy.getApps();
  }

  @override
  Future<void> setDomain(String domain, String serverHost) async {
    await proxy.setDomain(domain: domain, serverHost: serverHost);
    await saveConfig();
  }

  @override
  Future<void> removeDomain(String domain) async {
    await proxy.removeDomain(domain: domain);
    saveConfig();
  }

  @override
  Future<void> setApp(String app, String serverHost) async {
    await proxy.setApp(app: app, serverHost: serverHost);
    await saveConfig();
  }

  @override
  Future<void> removeApp(String app) async {
    await proxy.removeApp(app: app);
    saveConfig();
  }

  @override
  Future<bool> checkDomain(String domain) {
    return ProxyService.checkDomain(domain: domain);
  }

  @override
  Future<void> addServer(ServerConfig config) async {
    await proxy.addServer(config: config);
    saveConfig();
  }

  @override
  Future<void> updateServer(String origHost, ServerConfig newConfig) async {
    await proxy.updateServer(origHost: origHost, newConfig: newConfig);
    saveConfig();
  }

  @override
  Future<void> deleteServer(String host) async {
    await proxy.deleteServer(host: host);
    saveConfig();
  }

  @override
  Future<int> getTTFB(String server, String domain) async {
    return proxy.getTtfb(server: server, domain: domain);
  }

  @override
  Future<int> getProxyPort() async {
    return await proxy.getProxyPort();
  }

  @override
  Future<void> setProxyPort(int port) async {
    await proxy.setProxyPort(port: port);
    saveConfig();
  }

  @override
  Future<bool> getAutostart() {
    return ProxyService.getAutostart();
  }

  @override
  Future<void> setAutostart(bool enabled) {
    return ProxyService.setAutostart(enabled: enabled);
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
    ProxyService.log(message: message);
  }

  @override
  Future<List<LogLine>> getLog(BigInt? start, int limit) =>
      ProxyService.getLog(start: start, limit: BigInt.from(limit));

  Future<void> saveConfig() async {
    final cfg = await proxy.getConfig();
    String json = jsonEncode(
      cfg,
      toEncodable: (Object? value) => value is ProxyConfig
          ? proxyCofigToJson(value)
          : throw UnsupportedError('Saving proxy config: Cannot convert to JSON: $value'),
    );
    final prefs = SharedPreferencesAsync();
    prefs.setString(kConfigKey, json);
  }
}
