import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/services/router_service_impl.dart';
import 'package:covert_connect/src/services/router_service_mock.dart';
import 'package:get_it/get_it.dart';

final di = GetIt.instance;

void setupDI() {
  if (const bool.hasEnvironment("MOCK_SERVICE")) {
    di.registerSingletonAsync<RouterServiceBase>(() => RouterServiceMock.create());
  } else {
    di.registerSingletonAsync<RouterServiceBase>(() => RouterServiceImpl.create());
  }
  di.registerSingleton<AppStateService>(AppStateService());
}