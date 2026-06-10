import 'package:shared_preferences/shared_preferences.dart';

class OptionService {
  static const _keyShowTotalTransfer = "showTotalTransfer";

  bool _showTotalTransfer = true;

  bool get showTotalTransfer => _showTotalTransfer;

  Future<void> init() async {
    final prefs = await SharedPreferences.getInstance();
    _showTotalTransfer = prefs.getBool(_keyShowTotalTransfer) ?? _showTotalTransfer;
  }

  Future<void> setShowTotalTransfer(bool value) async {
    _showTotalTransfer = value;
    await _saveOptions();
  }

  Future<void> _saveOptions() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_keyShowTotalTransfer, _showTotalTransfer);
  }
}
