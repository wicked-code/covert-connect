import 'dart:convert';

import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';

const kStateKey = "state";
const kPortKey = "port";
const kDomainsKey = "domains";
const kServersKey = "servers";

const kCaption = "caption";
const kHost = "host";
const kWeight = "weight";
const kDomains = "domains";
const kEnabled = "enabled";
const kProtocol = "protocol";

const kKey = "key";
const kKdf = "kdf";
const kCipher = "cipher";
const kMaxConnectDelay = "maxConnectDelay";
const kHPaddingStart = "h_start";
const kHPaddingEnd = "h_end";
const kDPaddingMax = "d_max";
const kDPaddingRate = "d_rate";
const kEncryptionLimit = "encryptionLimit";

ProxyConfig proxyConfigFromString(String configStr) {
  final json = jsonDecode(configStr);

  return ProxyConfig(
    state: ProxyState.values.byName(json[kStateKey] as String),
    port: json[kPortKey] as int,
    domains: (json[kDomainsKey] as List<dynamic>).map((x) => x as String).toList(),
    servers: (json[kServersKey] as List<dynamic>).map((x) => serverConfigFromJson(x as Map<String, dynamic>)).toList(),
  );
}

ServerConfig serverConfigFromJson(Map<String, dynamic> json) {
  return ServerConfig(
    caption: json[kCaption] as String?,
    host: json[kHost] as String,
    weight: json[kWeight] as int?,
    domains: (json[kDomains] as List<dynamic>?)?.map((x) => x as String).toList(),
    enabled: json[kEnabled] as bool,
    protocol: protocolConfigFromJson(json[kProtocol] as Map<String, dynamic>),
  );
}

ProtocolConfig protocolConfigFromJson(Map<String, dynamic> json) {
  return ProtocolConfig(
    key: json[kKey] as String,
    kdf: Kdf.values.byName(json[kKdf] as String),
    cipher: CipherType.values.byName(json[kCipher] as String),
    maxConnectDelay: json[kMaxConnectDelay] as int,
    headerPadding: HeaderPadding(start: json[kHPaddingStart] as int, end: json[kHPaddingEnd] as int),
    dataPadding: DataPadding(max: json[kDPaddingRate] as int, rate: json[kDPaddingRate] as int),
    encryptionLimit: BigInt.parse(json[kEncryptionLimit] as String),
  );
}

Map<String, dynamic> proxyCofigToJson(ProxyConfig value) => {
  kStateKey: value.state.name,
  kPortKey: value.port,
  kDomainsKey: value.domains,
  kServersKey: value.servers.map((s) => serverConfigToJson(s)).toList(),
};

Map<String, dynamic> serverConfigToJson(ServerConfig value) => {
  if (value.caption case final caption?) kCaption: caption,
  kHost: value.host,
  if (value.weight case final weight?) kWeight: weight,
  if (value.domains case final domains?) kDomains: domains,
  kEnabled: value.enabled,
  kProtocol: protocolConfigToJson(value.protocol),
};

Map<String, dynamic> protocolConfigToJson(ProtocolConfig value) => {
  kKey: value.key,
  kKdf: value.kdf.name,
  kCipher: value.cipher.name,
  kMaxConnectDelay: value.maxConnectDelay,
  kHPaddingStart: value.headerPadding.start,
  kHPaddingEnd: value.headerPadding.end,
  kDPaddingMax: value.dataPadding.max,
  kDPaddingRate: value.dataPadding.rate,
  kEncryptionLimit: value.encryptionLimit.toString(),
};
