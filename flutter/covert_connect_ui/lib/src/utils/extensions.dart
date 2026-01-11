import 'package:punycoder/punycoder.dart';
const domainCodec = PunycodeCodec();

final domaianValidSymbolsRegex = RegExp(r'^(?!-)[a-z0-9-]{1,63}(?<!-)$');

extension StringCasingExtension on String {
  String get capitalized => length > 0 ?'${this[0].toUpperCase()}${substring(1).toLowerCase()}':'';
  String get titleCase => replaceAll(RegExp(' +'), ' ').split(' ').map((str) => str.capitalized).join(' ');
  bool get isDomainValidSymbolsOnly => domaianValidSymbolsRegex.hasMatch(this);
  String removeIfStartWith(String prefix) => startsWith(prefix) ? substring(prefix.length) : this;
  String decodePunycode() => domainCodec.decode(this);
  String encodePunycode() => domainCodec.encode(this);
}