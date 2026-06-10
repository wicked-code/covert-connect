import 'package:covert_connect/src/services/option_service.dart';
import 'package:covert_connect/src/services/router_service.dart';
import 'package:covert_connect/src/services/router_service_impl.dart';
import 'package:covert_connect/src/services/router_service_mock.dart';
import 'package:get_it/get_it.dart';

final di = GetIt.instance;

void setupDI() {
  if (const bool.hasEnvironment("MOCK_SERVICE")) {
    di.registerSingletonAsync<RouterServiceBase>(
      () => RouterServiceMock.create(),
      dispose: (service) => service.dispose(),
    );
  } else {
    di.registerSingletonAsync<RouterServiceBase>(
      () => RouterServiceImpl.create(),
      dispose: (service) => service.dispose(),
    );
  }
  di.registerSingletonAsync<OptionService>(
    () async {
      final service = OptionService();
      await service.init();
      return service;
    },
  );
}