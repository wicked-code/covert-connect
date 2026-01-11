import 'package:collection/collection.dart';
import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/extensions.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:covert_connect/src/widgets/hover.dart';
import 'package:covert_connect/src/widgets/text_with_tooltip.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class DomainInfo {
  const DomainInfo(this.domain, this.server);

  final String domain;
  final String server;
}

class DomainList extends StatefulWidget {
  const DomainList({super.key, required this.domains, required this.onDeleteDomain, required this.onEditDomain});

  final List<DomainInfo> domains;
  final ValueChanged<DomainInfo> onDeleteDomain;
  final ValueChanged<DomainInfo> onEditDomain;

  @override
  State<DomainList> createState() => _DomainListState();
}

class _DomainListState extends State<DomainList> {
  int _hoverIndex = -1;

  void _hoverRow(int index, bool hovering) {
    setState(() {
      _hoverIndex = hovering ? index : -1;
    });
  }

  void _edit(int index) {
    widget.onEditDomain(widget.domains[index]);
  }

  Color _highlightRow(Color color, Color highlightColor, int index) {
    if (index != _hoverIndex) {
      return color;
    }

    return Color.alphaBlend(highlightColor, color);
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
                      child: Text("Domain", style: headerTextStyle),
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
                    ...widget.domains.mapIndexed((idx, info) {
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
                            child: TextWithTooltip(info.domain.decodePunycode(), style: cellTextStyle),
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
                                  onPressed: () => widget.onDeleteDomain(info),
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
