import 'package:covert_connect/src/utils/color_utils.dart';
import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/widgets/hover.dart';
import 'package:covert_connect/src/widgets/text_with_tooltip.dart';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class AppInfo {
  final String path;
  final int pid;

  AppInfo({required this.path, required this.pid});

  @override
  String toString() => 'PID: $pid | Path: $path';
}

class AppList extends StatefulWidget {
  const AppList({super.key, required this.apps, required this.onSelect});

  final List<AppInfo> apps;
  final ValueChanged<AppInfo> onSelect;

  @override
  State<AppList> createState() => _AppListState();
}

class _AppListState extends State<AppList> {
  List<AppInfo> get apps => widget.apps;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;
    final borderRadius = BorderRadius.circular(8);
    final headerTextStyle = GoogleFonts.inter(fontSize: 14, fontWeight: FontWeight.w700);

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
            Container(
              decoration: BoxDecoration(color: colorScheme.surface),
              child: Row(
                children: [
                  Container(width: 14),
                  Expanded(
                    child: Padding(
                      padding: EdgeInsets.only(top: 9, bottom: 9),
                      child: Text("App", style: headerTextStyle),
                    ),
                  ),
                  SizedBox(width: 67, child: Text("PID", style: headerTextStyle)),
                ],
              ),
            ),
            Flexible(
              child: AnimatedSwitcher(
                duration: Durations.long1,
                switchInCurve: Curves.easeInOut,
                switchOutCurve: Curves.easeInOut,
                child: apps.isEmpty ? Container() : _AppTable(apps: apps, onSelect: widget.onSelect),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _AppTable extends StatefulWidget {
  const _AppTable({required this.apps, required this.onSelect});

  final List<AppInfo> apps;
  final ValueChanged<AppInfo> onSelect;

  @override
  State<_AppTable> createState() => _AppTableState();
}

class _AppTableState extends State<_AppTable> {
  List<AppInfo> get apps => widget.apps;

  int _hoverIndex = -1;

  void _hoverRow(int index, bool hovering) {
    final newIndex = hovering ? index : -1;
    if (_hoverIndex == newIndex) return;
    setState(() {
      _hoverIndex = newIndex;
    });
  }

  Color _highlightRow(Color color, Color highlightColor, int index) {
    if (index != _hoverIndex) {
      return color;
    }

    return Color.alphaBlend(highlightColor, color);
  }

  void _select(int index) {
    widget.onSelect(widget.apps[index]);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    final cellTextStyle = GoogleFonts.inter(fontSize: 13, fontWeight: FontWeight.w400);

    final selectedColor = colorScheme.primary.withValues(alpha: 0.05);

    Color rowColorEven = darken(colorScheme.surface, 0.95, 1.05, theme.brightness).withValues(alpha: 0.57);

    return CustomScrollView(
      slivers: <Widget>[
        SliverList(
          delegate: SliverChildBuilderDelegate((BuildContext context, int index) {
            final info = widget.apps[index];
            return Container(
              decoration: BoxDecoration(
                color: _highlightRow(index % 2 == 0 ? rowColorEven : colorScheme.surface, selectedColor, index),
                border: Border(top: BorderSide(color: theme.dividerColor, width: 1)),
              ),
              child: Row(
                children: [
                  SizedBox(
                    width: 14,
                    child: _Cell(
                      index: index,
                      onHover: _hoverRow,
                      onTap: _select,
                      child: Tooltip(
                        message: info.path,
                        child: Container(
                          padding: const EdgeInsets.only(right: 0.5),
                          height: 28,
                          child: Center(
                            child: buildSvg(
                              width: 8,
                              height: 8,
                              "assets/icons/open-file.svg",
                              color: Theme.of(context).colorScheme.tertiary.withValues(alpha: 0.4),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                  Expanded(
                    child: _Cell(
                      index: index,
                      onHover: _hoverRow,
                      onTap: _select,
                      child: TextWithTooltip(info.path.split('/').last.split(r'\').last, style: cellTextStyle),
                    ),
                  ),
                  SizedBox(
                    width: 67,
                    child: _Cell(
                      index: index,
                      onHover: _hoverRow,
                      onTap: _select,
                      child: Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Expanded(
                            child: Text(info.pid.toString(), overflow: TextOverflow.ellipsis, style: cellTextStyle),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            );
          }, childCount: widget.apps.length),
        ),
      ],
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
