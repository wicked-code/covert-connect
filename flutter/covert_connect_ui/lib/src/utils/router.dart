import 'package:flutter/cupertino.dart';

extension AppCupertinoNavigator on BuildContext {
  void cupertinoGoTo(Widget page) {
    Navigator.of(this).push(CupertinoPageRoute<void>(builder: (_) => page));
  }
}
