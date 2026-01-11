import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/services/proxy_service_impl.dart';
import 'package:covert_connect/src/services/proxy_service_mock.dart';
import 'package:get_it/get_it.dart';

final di = GetIt.instance;

void setupDI() {
  if (const bool.hasEnvironment("MOCK_SERVICE")) {
    di.registerSingletonAsync<ProxyServiceBase>(() => ProxyServiceMock.create());
  } else {
    di.registerSingletonAsync<ProxyServiceBase>(() => ProxyServiceImpl.create());
  }
  di.registerSingleton<AppStateService>(AppStateService());
}