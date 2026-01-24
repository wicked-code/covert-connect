import 'package:collection/collection.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/hover.dart';
import 'package:covert_connect/src/widgets/text_with_tooltip.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class RouteInfo {
  const RouteInfo(this.value, this.server);

  final String value;
  final String server;
}

class RouteList extends StatefulWidget {
  const RouteList({super.key, required this.routeName, required this.routes, required this.onDeleteRoute, required this.onEditRoute, this.decodeValue});

  final String routeName;
  final List<RouteInfo> routes;
  final ValueChanged<RouteInfo> onDeleteRoute;
  final ValueChanged<RouteInfo> onEditRoute;
  final String Function(String)? decodeValue;

  @override
  State<RouteList> createState() => _RouteListState();
}

class _RouteListState extends State<RouteList> {
  int _hoverIndex = -1;

  void _hoverRow(int index, bool hovering) {
    setState(() {
      _hoverIndex = hovering ? index : -1;
    });
  }

  void _edit(int index) {
    widget.onEditRoute(widget.routes[index]);
  }

  Color _highlightRow(Color color, Color highlightColor, int index) {
    if (index != _hoverIndex) {
      return color;
    }

    return Color.alphaBlend(highlightColor, color);
  }

  String _decodeValue(String value) {
    final decodeValue = widget.decodeValue;
    return decodeValue == null ? value : decodeValue(value);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final borderRadius = BorderRadius.circular(8);
    final headerTextStyle = GoogleFonts.inter(fontSize: 14, fontWeight: FontWeight.w700);
    final cellTextStyle = GoogleFonts.inter(fontSize: 13, fontWeight: FontWeight.w400);

    final selectedColor = colorScheme.primary.withValues(alpha: 0.05);

    double hPadding = isDesktop ? 7 : 0;

    Color rowColorEven = darken(colorScheme.surface, 0.95, 1.05, theme.brightness).withValues(alpha: 0.57);

    const columnSizes = <int, TableColumnWidth>{
      0: FixedColumnWidth(8),
      1: FlexColumnWidth(),
      2: FlexColumnWidth(),
      3: FixedColumnWidth(35),
    };

    return Container(
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
                    Container(),
                    Padding(
                      padding: EdgeInsets.only(top: 9, bottom: 9),
                      child: Text(widget.routeName, style: headerTextStyle),
                    ),
                    Align(
                      alignment: Alignment.centerLeft,
                      child: Text("Server", style: headerTextStyle),
                    ),
                    Container(),
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
                    ...widget.routes.mapIndexed((idx, info) {
                      return TableRow(
                        decoration: BoxDecoration(
                          color: _highlightRow(idx % 2 == 0 ? rowColorEven : colorScheme.surface, selectedColor, idx),
                          border: Border(top: BorderSide(color: theme.dividerColor, width: 1)),
                        ),
                        children: [
                          _Cell(index: idx, onHover: _hoverRow, onTap: _edit, child: Container()),
                          _Cell(
                            index: idx,
                            onHover: _hoverRow,
                            onTap: _edit,
                            child: TextWithTooltip(_decodeValue(info.value), style: cellTextStyle),
                          ),
                          _Cell(
                            index: idx,
                            onHover: _hoverRow,
                            onTap: _edit,
                            child: Row(
                              mainAxisAlignment: MainAxisAlignment.spaceBetween,
                              children: [
                                Expanded(
                                  child: TextWithTooltip(
                                    info.server.isNotEmpty ? info.server : "direct",
                                    style: cellTextStyle,
                                  ),
                                ),
                              ],
                            ),
                          ),
                          _Cell(
                            index: idx,
                            onHover: _hoverRow,
                            onTap: _edit,
                            child: Row(
                              children: [
                                AppIconButton(
                                  asset: "assets/icons/delete.svg",
                                  assetColor: Colors.redAccent,
                                  onPressed: () => widget.onDeleteRoute(info),
                                ),
                                SizedBox(width: hPadding),
                              ],
                            ),
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

class _Cell extends StatelessWidget {
  const _Cell({required this.index, required this.onHover, required this.onTap, required this.child});

  final int index;
  final void Function(int index, bool hovering) onHover;
  final void Function(int index) onTap;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Hover(
      onChange: (hovering) => onHover(index, hovering),
      child: GestureDetector(onTap: () => onTap(index), child: child),
    );
  }
}
