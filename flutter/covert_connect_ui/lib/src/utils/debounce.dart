import 'dart:async';

class Debounce {
  Debounce({required this.duration});
  final Duration duration;
  Timer? _timer;

  void debounce(void Function() action) {
    cancelDebounce();
    _timer = Timer(duration, () => action());
  }

  void cancelDebounce() {
    if (_timer?.isActive ?? false) _timer?.cancel();
  }
}