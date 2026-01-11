import 'package:flutter/cupertino.dart';

/// override CupertinoPageRoute to make background transparent
class AppCupertinoPageRoute<T> extends CupertinoPageRoute<T> {
  AppCupertinoPageRoute({required super.builder});

  @override
  Color? get barrierColor => null;
}
