import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/home.dart';
import 'package:covert_connect/src/utils/desktop/init.dart';
import 'package:covert_connect/src/widgets/app_cupertino_router.dart';
import 'package:covert_connect/src/widgets/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:covert_connect/src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await Future.wait([RustLib.init(), initDesktop()]);
  setupDI();

  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return AppTheme(
      builder: (context, theme, darkTheme) => MaterialApp(
        title: 'Covert Connect',
        debugShowCheckedModeBanner: false,
        // showPerformanceOverlay: true,
        // checkerboardRasterCacheImages: true,
        theme: theme,
        darkTheme: darkTheme,
        themeMode: ThemeMode.system,
        onGenerateRoute: (settings) {
          assert(settings.name == '/');
          return AppCupertinoPageRoute(builder: (context) => const HomePage());
        },
      ),
    );
  }
}
