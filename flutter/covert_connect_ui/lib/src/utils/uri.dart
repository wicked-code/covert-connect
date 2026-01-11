const kKeyParam = "k";
const kNameParam = "n";
const kScheme = "cc";

class UriData {
  const UriData({required this.host, required this.key, this.name});

  final String host;
  final String key;
  final String? name;
}

extension StringUriExtension on String {
  UriData? tryParseUri() {
    final parsed = Uri.parse(this);
    if (parsed.scheme != kScheme ||
        parsed.host.isEmpty ||
        !parsed.queryParameters.containsKey(kKeyParam) ||
        parsed.queryParameters[kKeyParam]!.isEmpty) {
      return null;
    }

    return UriData(
      host: "${parsed.host}${parsed.hasPort ? ":${parsed.port}" : ""}",
      key: parsed.queryParameters[kKeyParam]!,
      name: parsed.queryParameters[kNameParam],
    );
  }
}

String createUri({required String host, required String key, String? name}) {
  final nameParam = name?.isNotEmpty ?? false ? {kNameParam: name} : null;
  return Uri(
    scheme: kScheme,
    host: host.split(":").first,
    port: host.split(":").length > 1 ? int.parse(host.split(":")[1]) : null,
    queryParameters: {kKeyParam: key, ...?nameParam },
  ).toString();
}
