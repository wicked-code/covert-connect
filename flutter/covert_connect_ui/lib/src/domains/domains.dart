import 'package:collection/collection.dart';
import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/domains/widgets/add_domain.dart';
import 'package:covert_connect/src/widgets/route_list.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/utils/debounce.dart';
import 'package:covert_connect/src/utils/extensions.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:covert_connect/src/widgets/toast.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class DomainsPage extends StatefulWidget {
  const DomainsPage({super.key});

  @override
  State<DomainsPage> createState() => _DomainsPageState();
}

class _DomainsPageState extends State<DomainsPage> {
  List<RouteInfo> _domains = [];
  List<RouteInfo> _filtered = [];

  bool _urlError = false;
  bool _urlShowError = false;
  final Debounce _debounceError = Debounce(duration: Duration(milliseconds: 1500));

  static String _lastHostFromClipboard = "";

  final TextEditingController _inputController = TextEditingController();
  String get _inputValue => _inputController.text.encodePunycode();

  Future<void> _loadDomains() async {
    final [rootDomains as List<String>, state as ProxyStateFull] = await Future.wait([
      di<ProxyServiceBase>().getDomains(),
      di<ProxyServiceBase>().getStateFull(),
    ]);
    _domains = rootDomains.map((d) => RouteInfo(d, "")).toList();
    for (final srv in state.servers) {
      _domains.addAll(srv.config.domains?.map((d) => RouteInfo(d, srv.config.host)) ?? []);
    }

    _filterDomains(_inputValue);
    _updateIfMounted();
  }

  void _addDomain() async {
    if (_urlError || _inputValue.isEmpty) return;

    final state = await di<ProxyServiceBase>().getStateFull();
    if (!mounted) return;

    final result = await AddDomainDialog.show(context, domain: _inputValue, servers: state.servers);

    if (result == true) {
      _inputController.clear();
      await _loadDomains();
    }
  }

  void _editDomain(RouteInfo info) async {
    final state = await di<ProxyServiceBase>().getStateFull();
    if (!mounted) return;

    final result = await AddDomainDialog.show(
      context,
      domain: info.value,
      servers: state.servers,
      selectedServer: info.server,
    );
    if (result == true) {
      await _loadDomains();
    }
  }

  void _deleteDomain(RouteInfo info) async {
    await di<ProxyServiceBase>().removeDomain(info.value);
    await _loadDomains();
    if (!mounted) return;

    Toast.warning(
      context,
      caption: info.value,
      text: "was removed, ${isDesktop ? 'click to undo' : 'tap to undo'}",
      onTap: () async {
        await di<ProxyServiceBase>().setDomain(info.value, info.server);
        await _loadDomains();
      },
    );
  }

  void _inputChanged(String inputValue) {
    String value = inputValue.encodePunycode();
    _tryParseUrl(value, (host) {
      _inputController.text = host.decodePunycode().removeIfStartWith("www.");
      value = _inputValue;
    });

    final domains = value.split('.');
    _urlError =
        value.length > 63 ||
        domains.any((d) => !(d.isNotEmpty && d.isDomainValidSymbolsOnly && !d.startsWith("-"))) ||
        ((domains.lastOrNull?.length ?? 0) < 2);

    if (!_urlError || value.isEmpty) {
      _debounceError.cancelDebounce();
      _urlShowError = false;
    } else {
      _debounceError.debounce(() {
        _urlShowError = _urlError;
        _updateIfMounted();
      });
    }

    _filterDomains(value);
    _updateIfMounted();
  }

  void _filterDomains(String value) {
    if (value.isNotEmpty) {
      _filtered = _domains.where((d) => d.value.contains(value)).toList();
    } else {
      _filtered = _domains;
    }
  }

  void _checkClipboard() async {
    ClipboardData? data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text == null) return;

    _tryParseUrl(data!.text!, (host) async {
      final reducedHost = host.split(".").reversed.take(3).toList().reversed.join(".").removeIfStartWith("www.");
      if (reducedHost == _lastHostFromClipboard) return;
      if (!(await di<ProxyServiceBase>().checkDomain(reducedHost))) return;

      _lastHostFromClipboard = reducedHost;
      _inputController.text = reducedHost;
    });
  }

  void _tryParseUrl(String url, ValueChanged<String> onParsed) {
    try {
      final uri = Uri.parse(url);
      if (uri.hasScheme && uri.hasAuthority) {
        onParsed(uri.host);
      }
    } catch (_) {}
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _inputController.dispose();
    super.dispose();
  }

  @override
  void initState() {
    _loadDomains();
    _checkClipboard();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final inputIsNotEmpty = _inputValue.isNotEmpty;
    return Padding(
      padding: EdgeInsets.all(8),
      child: Column(
        spacing: 8,
        children: [
          Row(
            spacing: 2,
            children: [
              Expanded(
                child: Padding(
                  padding: EdgeInsets.symmetric(vertical: 4),
                  child: Input(
                    hint: "Enter domain...",
                    keyboardType: TextInputType.url,
                    controller: _inputController,
                    onChanged: _inputChanged,
                    error: inputIsNotEmpty && _urlShowError,
                    icon: _inputValue.isNotEmpty
                        ? SizedBox(
                            height: 20,
                            width: 20,
                            child: AppIconButton(
                              asset: "assets/icons/delete.svg",
                              assetColor: Colors.redAccent,
                              onPressed: () => setState(() {
                                _inputController.clear();
                                _filterDomains("");
                              }),
                            ),
                          )
                        : null,
                  ),
                ),
              ),
              AppIconButton(asset: "assets/icons/plus.svg", onPressed: inputIsNotEmpty ? _addDomain : null),
            ],
          ),
          Flexible(
            child: RouteList(
              routeName: "Domain",
              routes: _filtered,
              onDeleteRoute: _deleteDomain,
              onEditRoute: _editDomain,
              decodeValue: (value) => value.decodePunycode(),
            ),
          ),
        ],
      ),
    );
  }
}
