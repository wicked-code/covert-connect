import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';
import 'package:covert_connect/src/services/router_service.dart';

class RouterServiceImpl implements RouterServiceBase {
  static Future<RouterServiceBase> create() async {
    final router = RouterServiceImpl();
    await router.init();
    return router;
  }

  final router = ClientService();

  Future<void> init() async {
    await router.start();
  }

  @override
  Future<ClientState> getState() => router.getState();

  @override
  Future<ClientStatus> getStatus() => router.getStatus();

  @override
  Future<void> setState(ClientState state) async {
    await router.setState(state: state);
  }

  @override
  Future<void> setServerEnabled(String host, bool value) async {
    await router.setServerEnabled(host: host, value: value);
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
  }

  @override
  Future<void> removeDomain(String domain) async {
    await router.removeDomain(domain: domain);
  }

  @override
  Future<void> setApp(String app, String serverHost) async {
    await router.setApp(app: app, serverHost: serverHost);
  }

  @override
  Future<void> removeApp(String app) async {
    await router.removeApp(app: app);
  }

  @override
  Future<bool> checkDomain(String domain) {
    return ClientService.checkDomain(domain: domain);
  }

  @override
  Future<void> addServer(ServerConfig config) async {
    await router.addServer(config: config);
  }

  @override
  Future<void> updateServer(String origHost, ServerConfig newConfig) async {
    await router.updateServer(origHost: origHost, newConfig: newConfig);
  }

  @override
  Future<void> deleteServer(String host) async {
    await router.deleteServer(host: host);
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
  Future<List<LogLine>> getLog(BigInt? start, BigInt? end, int limit) =>
      ClientService.getLog(start: start, end: end, limit: BigInt.from(limit));
}
