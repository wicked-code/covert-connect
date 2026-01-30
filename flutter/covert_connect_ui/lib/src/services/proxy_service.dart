import 'package:covert_connect/src/rust/api/log.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';

enum LogErrorType {
  message,
  warning,
  error
}

abstract class ProxyServiceBase {
  Future<ProxyStateFull> getStateFull();
  Future<ProxyState> getProxyState();
  Future<void> setProxyState(ProxyState state);
  Future<void> setServerEnabled(String host, bool value);
  Future<ProtocolConfig> getServerProtocol(String host, String key);
  Future<List<String>> getDomains();
  Future<List<String>> getApps();
  Future<void> setDomain(String domain, String serverHost);
  Future<void> removeDomain(String domain);
  Future<bool> checkDomain(String domain);
  Future<void> setApp(String app, String serverHost);
  Future<void> removeApp(String app);
  Future<void> addServer(ServerConfig newConfig);
  Future<void> updateServer(String origHost, ServerConfig newConfig);
  Future<void> deleteServer(String host);
  Future<int> getTTFB(String server, String domain);
  Future<void> log(String message, {LogErrorType? type});
  Future<int> getProxyPort();
  Future<void> setProxyPort(int port);
  Future<bool> getAutostart();
  Future<void> setAutostart(bool enabled);
  Future<List<LogLine>> getLog(BigInt? start, int limit);
  Future<BigInt> registerLogger(Future<void> Function(String) callback);
  Future<void> unregisterLogger(BigInt id);
}
