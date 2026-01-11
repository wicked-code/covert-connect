import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/status/add_edit_server.dart';
import 'package:covert_connect/src/status/widgets/server_select_mode.dart';
import 'package:covert_connect/src/status/widgets/server_state_indicator.dart';
import 'package:covert_connect/src/status/widgets/traffic_cell.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/router.dart';
import 'package:covert_connect/src/utils/uri.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/text_with_tooltip.dart';
import 'package:covert_connect/src/widgets/toast.dart';
import 'package:flutter/material.dart';
import 'package:collection/collection.dart';
import 'package:flutter/services.dart';
import 'package:google_fonts/google_fonts.dart';

class ServerList extends StatefulWidget {
  const ServerList({super.key, required this.servers, required this.updateServers});

  final List<ServerInfo> servers;
  final VoidCallback updateServers;

  @override
  State<ServerList> createState() => _ServerListState();
}

class _ServerListState extends State<ServerList> {
  List<ServerInfo> prevServers = [];
  final serverSelectModeController = ServerSelectModeController();

  void _toggleServer(String host, bool value) async {
    if (serverSelectModeController.mode == ServerSelectMode.toggle) {
      await Future.wait([
        ...widget.servers.map((server) {
          bool useValue = server.config.host == host ? value : false;
          return di<ProxyServiceBase>().setServerEnabled(server.config.host, useValue);
        }),
      ]);
    } else {
      await di<ProxyServiceBase>().setServerEnabled(host, value);
    }
    widget.updateServers();
  }

  void _share(ServerInfo server) async {
    String uri = createUri(host: server.config.host, key: server.config.protocol.key, name: server.config.caption);
    await Clipboard.setData(ClipboardData(text: uri));
    if (!mounted) return;
    Toast.success(context, caption: 'Share Server', text: "URI copied to clipboard");
  }

  @override
  void didUpdateWidget(covariant ServerList oldWidget) {
    if (oldWidget.servers != widget.servers) {
      prevServers = oldWidget.servers;
    }
    super.didUpdateWidget(oldWidget);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final borderRadius = BorderRadius.circular(8);

    final thinTextStyle = GoogleFonts.inter(
      fontSize: 11,
      fontWeight: FontWeight.w300,
      height: 1.0,
      color: colorScheme.onSurface.withValues(alpha: 0.57),
    );
    final headerTextStyle = GoogleFonts.inter(fontSize: 14, fontWeight: FontWeight.w700);
    final cellTextStyle = GoogleFonts.inter(fontSize: 13, fontWeight: FontWeight.w400);

    String getServerIpPort(ServerInfo server) => "${server.ip}:${server.port}";

    String getServerHost(ServerInfo server) {
      if (server.config.caption?.isNotEmpty ?? false) {
        return server.config.caption!;
      }

      if (server.config.host.isNotEmpty && server.config.host != getServerIpPort(server)) {
        return server.config.host.split(':').first;
      }

      return '-';
    }

    double hPadding = isDesktop ? 7 : 0;

    Color rowColorEven = darken(colorScheme.surface, 0.95, 1.05, theme.brightness).withValues(alpha: 0.57);

    const columnSizes = <int, TableColumnWidth>{
      0: FixedColumnWidth(40),
      1: FlexColumnWidth(),
      2: FixedColumnWidth(115),
      3: FixedColumnWidth(63),
    };
    return Container(
      margin: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        borderRadius: borderRadius,
        border: Border.all(color: theme.dividerColor, width: 1),
      ),
      child: ClipRRect(
        borderRadius: borderRadius,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Table(
              columnWidths: columnSizes,
              defaultVerticalAlignment: TableCellVerticalAlignment.middle,
              children: [
                TableRow(
                  decoration: BoxDecoration(color: colorScheme.surface),
                  children: [
                    ServerSelectModeMenu(controller: serverSelectModeController),
                    Padding(
                      padding: EdgeInsets.only(top: 9, bottom: 9),
                      child: Text("Server", style: headerTextStyle),
                    ),
                    Center(child: Text("Traffic", style: headerTextStyle)),
                    Align(
                      alignment: Alignment.centerRight,
                      child: Padding(
                        padding: EdgeInsets.only(right: hPadding),
                        child: AppIconButton(
                          asset: "assets/icons/plus.svg",
                          onPressed: () => context.cupertinoGoTo(AddEditServerPage()),
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
            Flexible(
              child: SingleChildScrollView(
                child: Table(
                  columnWidths: columnSizes,
                  defaultVerticalAlignment: TableCellVerticalAlignment.middle,
                  children: [
                    ...widget.servers.mapIndexed((idx, server) {
                      final oldServer = prevServers.firstWhereOrNull((s) => s.config.host == server.config.host);
                      final rxChanged = (server.state.rxTotal - (oldServer?.state.rxTotal ?? BigInt.zero)).abs();
                      final txChanged = (server.state.txTotal - (oldServer?.state.txTotal ?? BigInt.zero)).abs();

                      return TableRow(
                        decoration: BoxDecoration(
                          color: idx % 2 == 0 ? rowColorEven : colorScheme.surface,
                          border: Border(top: BorderSide(color: theme.dividerColor, width: 1)),
                        ),
                        children: [
                          ServerStateIndicator(
                            server: server,
                            onTap: () => _toggleServer(server.config.host, !server.config.enabled),
                          ),
                          Padding(
                            padding: EdgeInsets.only(top: 6, bottom: 6, right: 8),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                TextWithTooltip(getServerHost(server), style: cellTextStyle),
                                Text(getServerIpPort(server), style: thinTextStyle),
                              ],
                            ),
                          ),
                          TrafficCell(
                            state: server.state,
                            rxChanged: rxChanged,
                            txChanged: txChanged,
                            style: cellTextStyle,
                            subStyle: thinTextStyle,
                          ),
                          Row(
                            children: [
                              AppIconButton(asset: "assets/icons/share.svg", onPressed: () => _share(server)),
                              AppIconButton(
                                asset: "assets/icons/edit.svg",
                                onPressed: () => context.cupertinoGoTo(AddEditServerPage(server: server)),
                              ),
                              SizedBox(width: hPadding),
                            ],
                          ),
                        ],
                      );
                    }),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
