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

class AppsPage extends StatefulWidget {
  const AppsPage({super.key});

  @override
  State<AppsPage> createState() => _AppsPageState();
}

class _AppsPageState extends State<AppsPage> {
  List<RouteInfo> _apps = [];
  List<RouteInfo> _filtered = [];

  bool _urlError = false;
  bool _urlShowError = false;
  final Debounce _debounceError = Debounce(duration: Duration(milliseconds: 1500));

  final TextEditingController _inputController = TextEditingController();
  String get _inputValue => _inputController.text.encodePunycode();

  Future<void> _loadDomains() async {
    // TODO: ??? add apps
    // final [rootDomains as List<String>, state as ProxyStateFull] = await Future.wait([
    //   di<ProxyServiceBase>().getDomains(),
    //   di<ProxyServiceBase>().getStateFull(),
    // ]);
    // _apps = rootDomains.map((d) => RouteInfo(d, "")).toList();
    // for (final srv in state.servers) {
    //   _apps.addAll(srv.config.domains?.map((d) => RouteInfo(d, srv.config.host)) ?? []);
    // }

    _filterApps(_inputValue);
    _updateIfMounted();
  }

  void _addApp() async {
    if (_urlError || _inputValue.isEmpty) return;

    // TODO: ???
    // final state = await di<ProxyServiceBase>().getStateFull();
    // if (!mounted) return;

    // final result = await AddDomainDialog.show(context, domain: _inputValue, servers: state.servers);

    // if (result == true) {
    //   _inputController.clear();
    //   await _loadDomains();
    // }
  }

  void _editApp(RouteInfo info) async {
    final state = await di<ProxyServiceBase>().getStateFull();
    if (!mounted) return;

    // TODO: ??? Edit app
    // final result = await AddDomainDialog.show(
    //   context,
    //   domain: info.domain,
    //   servers: state.servers,
    //   selectedServer: info.server,
    // );
    // if (result == true) {
    //   await _loadDomains();
    // }
  }

  void _deleteApp(RouteInfo info) async {
    // TODO: ??? remove route
    // await di<ProxyServiceBase>().removeDomain(info.domain);
    // await _loadDomains();
    // if (!mounted) return;

    // Toast.warning(
    //   context,
    //   caption: info.domain,
    //   text: "was removed, ${isDesktop ? 'click to undo' : 'tap to undo'}",
    //   onTap: () async {
    //     await di<ProxyServiceBase>().setDomain(info.domain, info.server);
    //     await _loadDomains();
    //   },
    // );
  }

  void _inputChanged(String inputValue) {
    String value = inputValue;
    // TODO: check for valid application name
    // if (!_urlError || value.isEmpty) {
    //   _debounceError.cancelDebounce();
    //   _urlShowError = false;
    // } else {
    //   _debounceError.debounce(() {
    //     _urlShowError = _urlError;
    //     _updateIfMounted();
    //   });
    // }

    _filterApps(value);
    _updateIfMounted();
  }

  void _filterApps(String value) {
    if (value.isNotEmpty) {
      _filtered = _apps.where((d) => d.value.contains(value)).toList();
    } else {
      _filtered = _apps;
    }
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
                    hint: "Enter app name...",
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
                                _filterApps("");
                              }),
                            ),
                          )
                        : null,
                  ),
                ),
              ),
              AppIconButton(asset: "assets/icons/plus.svg", onPressed: inputIsNotEmpty ? _addApp : null),
            ],
          ),
          Flexible(
            child: RouteList(routeName: "App", routes: _filtered, onDeleteRoute: _deleteApp, onEditRoute: _editApp),
          ),
        ],
      ),
    );
  }
}
