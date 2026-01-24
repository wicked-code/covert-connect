import 'package:flutter/cupertino.dart';

extension AppCupertinoNavigator on BuildContext {
  void cupertinoGoTo(Widget page) {
    Navigator.of(this).push(CupertinoPageRoute<void>(builder: (_) => page));
  }

  void slideGoTo(Widget page, [RouteTransition transition = RouteTransition.fromRightToLeft]) {
    Navigator.of(this).push(buildRouteSlide<void>(page, transition));
  }
}

enum RouteTransition { fromLeftToRight, fromRightToLeft }

Offset _offsetFromTransition(RouteTransition transition) {
  switch (transition) {
    case RouteTransition.fromLeftToRight:
      return Offset(-1.0, 0.0);
    case RouteTransition.fromRightToLeft:
      return Offset(1.0, 0.0);
  }
}

Route<T> buildRouteSlide<T>(Widget page, RouteTransition transition) {
  return _CustomPageRouteBuilder(
    transitionDuration: Duration(milliseconds: 500),
    reverseTransitionDuration: Duration(milliseconds: 500),
    pageBuilder: (context, animation, secondaryAnimation) => page,
    transitionsBuilder: (context, animation, secondaryAnimation, child) {
      var begin = _offsetFromTransition(transition);
      const end = Offset.zero;
      final curve = CurveTween(curve: Curves.ease);

      var tweenEnter = Tween(begin: begin, end: end).chain(curve);

      return SlideTransition(position: animation.drive(tweenEnter), child: child);
    },
    prevTransitionBuilder: (context, animation, secondaryAnimation, bool allowSnapshotting, child) {
      const begin = Offset.zero;
      var end = -_offsetFromTransition(transition);
      final curve = CurveTween(curve: Curves.ease);

      var tweenExit = Tween(begin: begin, end: end).chain(curve);

      return SlideTransition(position: secondaryAnimation.drive(tweenExit), child: child);
    },
  );
}

class _CustomPageRouteBuilder<T> extends PageRoute<T> {
  /// Creates a route that delegates to builder callbacks.
  _CustomPageRouteBuilder({
    super.settings,
    super.requestFocus,
    required this.pageBuilder,
    required this.transitionsBuilder,
    required this.prevTransitionBuilder,
    this.transitionDuration = const Duration(milliseconds: 300),
    this.reverseTransitionDuration = const Duration(milliseconds: 300),
    this.opaque = true,
    this.barrierDismissible = false,
    this.barrierColor,
    this.barrierLabel,
    this.maintainState = true,
    super.fullscreenDialog,
    super.allowSnapshotting = true,
  });

  /// {@template flutter.widgets.pageRouteBuilder.pageBuilder}
  /// Used build the route's primary contents.
  ///
  /// See [ModalRoute.buildPage] for complete definition of the parameters.
  /// {@endtemplate}
  final RoutePageBuilder pageBuilder;

  /// {@template flutter.widgets.pageRouteBuilder.transitionsBuilder}
  /// Used to build the route's transitions.
  ///
  /// See [ModalRoute.buildTransitions] for complete definition of the parameters.
  /// {@endtemplate}
  ///
  /// The default transition is a jump cut (i.e. no animation).
  final RouteTransitionsBuilder transitionsBuilder;

  @override
  final Duration transitionDuration;

  @override
  final Duration reverseTransitionDuration;

  @override
  final bool opaque;

  @override
  final bool barrierDismissible;

  @override
  final Color? barrierColor;

  @override
  final String? barrierLabel;

  @override
  final bool maintainState;

  @override
  DelegatedTransitionBuilder? get delegatedTransition => prevTransitionBuilder;

  DelegatedTransitionBuilder prevTransitionBuilder;

  @override
  Widget buildPage(BuildContext context, Animation<double> animation, Animation<double> secondaryAnimation) {
    return pageBuilder(context, animation, secondaryAnimation);
  }

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
    return transitionsBuilder(context, animation, secondaryAnimation, child);
  }
}
