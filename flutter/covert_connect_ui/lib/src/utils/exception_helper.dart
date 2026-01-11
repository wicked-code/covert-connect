import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

String exceptionToString(dynamic e) {
  if (e is AnyhowException) {
    return e.message;
  }
  if (e is PanicException) {
    return e.message;
  }
  if (e.message != null) {
    return e.message;
  }

  return e.toString();
}