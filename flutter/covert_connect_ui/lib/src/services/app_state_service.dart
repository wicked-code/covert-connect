import 'package:flutter/cupertino.dart';

enum AppState { hidden, visible}

class AppStateService extends ValueNotifier<AppState> {
  AppStateService() : super(AppState.hidden);
}