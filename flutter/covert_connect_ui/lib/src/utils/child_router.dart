import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_cupertino_router.dart';
import 'package:flutter/cupertino.dart';

final appChildNavigator = AppChildNavigator();

class ChildNavigator extends StatefulWidget {
  const ChildNavigator({super.key, required this.selected, required this.builder});

  final WidgetBuilder builder;
  final bool selected;

  @override
  State<ChildNavigator> createState() => ChildNavigatorState();
}

class ChildNavigatorState extends State<ChildNavigator> {
  final GlobalKey<NavigatorState> navigatorKey = GlobalKey<NavigatorState>();
  final AppNavigatorObserver routeObserver = AppNavigatorObserver();

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) => routeObserver.active = widget.selected);
    return PopScope(
      canPop: false, // Initially disable system back gestures
      onPopInvokedWithResult: (bool didPop, Object? result) async {
        if (didPop) {
          return; // Pop already happened, do nothing
        }
        final NavigatorState? childNavigator = navigatorKey.currentState;
        if (childNavigator != null && childNavigator.canPop()) {
          childNavigator.pop();
        } else {
          Navigator.of(context).pop();
        }
      },
      child: Navigator(
        key: navigatorKey,
        observers: [if (isDesktop) routeObserver],
        onGenerateRoute: (settings) {
          return AppCupertinoPageRoute(builder: (context) => widget.builder(context));
        },
      ),
    );
  }
}

abstract mixin class AppRouteAware {
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {}
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {}
  void didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) {}
  void didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) {}
  void didChangeTop(Route<dynamic> topRoute, Route<dynamic>? previousTopRoute) {}
  void didChangeNavigator(NavigatorState? navigator) {}
}

class AppChildNavigator {
  final List<AppRouteAware> _observers = [];
  AppNavigatorObserver? _observer;

  void subscribe(AppRouteAware observer) {
    _observers.add(observer);
  }

  void unsubscribe(AppRouteAware observer) {
    _observers.remove(observer);
  }

  void pop() {
    _observer?.navigator?.pop();
  }

  bool canPop() {
    return _observer?.navigator?.canPop() ?? false;
  }

  void _setActiveObserver(AppNavigatorObserver observer, bool active) {
    if (!active) {
      if (_observer == observer) {
        _observer = null;
        _navigatorChanged();
      }
    } else {
      _observer = observer;
      _navigatorChanged();
    }
  }

  void _navigatorChanged() {
    for (var observer in _observers) {
      observer.didChangeNavigator(_observer?.navigator);
    }
  }

  void _didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    for (var observer in _observers) {
      observer.didPush(route, previousRoute);
    }
  }

  void _didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {
    for (var observer in _observers) {
      observer.didPop(route, previousRoute);
    }
  }

  void _didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) {
    for (var observer in _observers) {
      observer.didRemove(route, previousRoute);
    }
  }

  void _didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) {
    for (var observer in _observers) {
      observer.didReplace(newRoute: newRoute, oldRoute: oldRoute);
    }
  }

  void _didChangeTop(Route<dynamic> topRoute, Route<dynamic>? previousTopRoute) {
    for (var observer in _observers) {
      observer.didChangeTop(topRoute, previousTopRoute);
    }
  }
}

class AppNavigatorObserver extends NavigatorObserver {
  set active(bool value) {
    appChildNavigator._setActiveObserver(this, value);
  }

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    appChildNavigator._didPush(route, previousRoute);
  }

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {
    appChildNavigator._didPop(route, previousRoute);
  }

  @override
  void didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) {
    appChildNavigator._didRemove(route, previousRoute);
  }

  @override
  void didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) {
    appChildNavigator._didReplace(newRoute: newRoute, oldRoute: oldRoute);
  }

  @override
  void didChangeTop(Route<dynamic> topRoute, Route<dynamic>? previousTopRoute) {
    appChildNavigator._didChangeTop(topRoute, previousTopRoute);
  }
}
