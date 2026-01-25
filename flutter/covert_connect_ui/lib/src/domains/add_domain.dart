import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/utils/extensions.dart';
import 'package:covert_connect/src/widgets/button.dart';
import 'package:covert_connect/src/widgets/hover.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

const int kSamplesCount = 3;
const int kDelayBetweenSamplesMs = 100;
const String kDirectHost = "Direct";

class AddDomainDialog extends StatefulWidget {
  const AddDomainDialog({super.key, required this.domain, required this.servers, this.selectedServer});

  final String domain;
  final List<ServerInfo> servers;
  final String? selectedServer;

  static Future<bool?> show(
    BuildContext context, {
    required String domain,
    required List<ServerInfo> servers,
    String? selectedServer,
  }) => showDialog<bool>(
    context: context,
    builder: (_) => AddDomainDialog(domain: domain, servers: servers, selectedServer: selectedServer),
  );

  @override
  State<AddDomainDialog> createState() => _AddDomainDialogState();
}

class _AddDomainDialogState extends State<AddDomainDialog> {
  List<ServerInfo> get servers => widget.servers;

  String _selected = kDirectHost;
  Map<String, ({double ping, int count})> pingMap = {};

  void _selectServer() {
    if (_selected != widget.selectedServer) {
      di<ProxyServiceBase>().setDomain(widget.domain, _selected == kDirectHost ? "" : _selected);
      if (mounted) {
        Navigator.of(context).pop(true);
      }
    }
  }

  void _init() async {
    for (int i = 0; i < kSamplesCount; i++) {
      for (final server in servers) {
        final host = server.config.host;
        di<ProxyServiceBase>().getTTFB(host, widget.domain).then((ping) {
          if (pingMap.containsKey(host)) {
            final old = pingMap[host]!;
            final newCount = old.count + 1;
            final newPing = (old.ping * old.count + ping) / newCount;
            pingMap[host] = (ping: newPing, count: newCount);
          } else {
            pingMap[host] = (ping: ping.toDouble(), count: 1);
          }
          if (!mounted) return;
          setState(() {});
        });
      }
      await Future.delayed(Duration(milliseconds: kDelayBetweenSamplesMs));
    }
  }

  @override
  void initState() {
    _selected = widget.selectedServer?.isNotEmpty == true ? widget.selectedServer! : kDirectHost;
    _init();
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final textTheme = theme.textTheme;
    final thinTextStyle = GoogleFonts.inter(
      fontSize: 14,
      fontWeight: FontWeight.w300,
      fontStyle: FontStyle.italic,
      height: 1.0,
      color: theme.colorScheme.onSurface.withValues(alpha: 0.63),
    );
    return Center(
      child: Container(
        margin: EdgeInsets.all(16),
        padding: EdgeInsets.all(16),
        decoration: BoxDecoration(color: theme.colorScheme.surface, borderRadius: BorderRadius.circular(8)),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              "${widget.selectedServer == null ? "Add" : "Edit"} domain",
              style: textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
            ),
            RichText(
              text: TextSpan(
                style: textTheme.labelLarge?.copyWith(color: textTheme.labelLarge?.color?.withValues(alpha: 0.75)),
                children: [
                  TextSpan(text: "select server for: "),
                  TextSpan(
                    text: widget.domain.decodePunycode(),
                    style: thinTextStyle,
                  ),
                ],
              ),
            ),
            SizedBox(height: 16),
            ListView(
              shrinkWrap: true,
              children: [
                _Server(
                  host: kDirectHost,
                  selected: _selected == kDirectHost,
                  onSelect: () => setState(() => _selected = kDirectHost),
                ),
                ...servers.map(
                  (server) => _Server(
                    host: server.config.host,
                    ping: pingMap[server.config.host]?.ping ?? 0,
                    count: pingMap[server.config.host]?.count ?? 0,
                    selected: _selected == server.config.host,
                    onSelect: () => setState(() => _selected = server.config.host),
                  ),
                ),
              ],
            ),
            SizedBox(height: 16),
            Row(
              spacing: 8,
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Button(label: "Cancel", onTap: () => Navigator.of(context).pop(false)),
                Button(label: "Select", onTap: _selectServer, primary: true),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _Server extends StatelessWidget {
  const _Server({required this.selected, required this.host, this.ping, this.count, required this.onSelect});

  final bool selected;
  final String host;
  final double? ping;
  final int? count;
  final VoidCallback onSelect;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final textTheme = theme.textTheme;
    final selectTextColor = selected ? colorScheme.onSecondary : colorScheme.primary;
    final textStyle = textTheme.bodyMedium?.copyWith(fontSize: 13, fontWeight: FontWeight.w500, color: selectTextColor);
    final pingStyle = count == null || count == kSamplesCount
        ? textStyle
        : textStyle?.copyWith(color: theme.disabledColor);

    return HoverBuilder(
      builder: (_, isHovering) => GestureDetector(
        onTap: onSelect,
        child: Container(
          decoration: BoxDecoration(
            color: selected
                ? colorScheme.secondary
                : isHovering
                ? colorScheme.secondary.withValues(alpha: 0.33)
                : null,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Padding(
            padding: EdgeInsets.symmetric(vertical: 3, horizontal: 8),
            child: Row(
              children: [
                Expanded(
                  child: Text(host, overflow: TextOverflow.ellipsis, style: textStyle),
                ),
                if (ping != null)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text("ping: ", style: textStyle),
                      SizedBox(
                        width: 27,
                        child: Center(child: Text("${ping! > 0 ? ping?.toStringAsFixed(0) : "..."}", style: pingStyle)),
                      ),
                      Text(" ms", style: textStyle),
                    ],
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
