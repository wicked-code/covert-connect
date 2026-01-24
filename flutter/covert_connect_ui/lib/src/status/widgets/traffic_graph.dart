import 'package:collection/collection.dart';
import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/services/app_state_service.dart';
import 'package:covert_connect/src/utils/graph_interval.dart';
import 'package:covert_connect/src/utils/text_utils.dart';
import 'package:flutter/material.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/scheduler.dart';
import 'package:google_fonts/google_fonts.dart';

const kPadding = 8.0;
const kLeftAxisTitleSize = 63.0;

const kUpdateIntervalMs = 500;
const kUpdateInterval = Duration(milliseconds: kUpdateIntervalMs);

const kTrafficGraphSamplesCount = 150;
const txColor = Color(0xFF800000);
const rxColor = Color(0xFF808000);

class TrafficSample {
  const TrafficSample({required this.rx, required this.tx, required this.time});
  final double rx, tx;
  final double time;
}

class TrafficGraph extends StatefulWidget {
  const TrafficGraph({super.key, required this.data, required this.height});

  final List<TrafficSample> data;
  final double height;

  @override
  State<TrafficGraph> createState() => _TrafficGraphState();
}

class _TrafficGraphState extends State<TrafficGraph> with TickerProviderStateMixin {
  late Ticker _ticker;

  List<TrafficSample> get data => widget.data;
  double _offsetX = 0.0;
  bool _smoothEnabled = true;

  @override
  void didUpdateWidget(covariant TrafficGraph oldWidget) {
    if (_ticker.isActive) {
      _ticker.stop();
    }
    _offsetX = 0;
    if (_smoothEnabled) {
      _ticker.start();
    }
    super.didUpdateWidget(oldWidget);
  }

  void _onAppStateChange() {
    _smoothEnabled = di<AppStateService>().value == AppState.visible;
  }

  @override
  void initState() {
    super.initState();
    di<AppStateService>().addListener(_onAppStateChange);
    _onAppStateChange();
    _ticker = createTicker((Duration elapsed) {
      int useElapsed = elapsed.inMilliseconds;
      _offsetX = useElapsed.toDouble();
      if (_offsetX >= kUpdateIntervalMs.toDouble()) {
        _offsetX = kUpdateIntervalMs.toDouble();
      }
      setState(() {});
    });
  }

  @override
  void deactivate() {
    _ticker.stop();
    super.deactivate();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final mediaQueryResult = MediaQuery.of(context);
    final width = mediaQueryResult.size.width;
    final graphWidth = width - kPadding - kLeftAxisTitleSize;
    final stepWidth = graphWidth / kTrafficGraphSamplesCount;

    const kFontSize = 11.0;
    final thinTextStyle = GoogleFonts.inter(
      fontSize: kFontSize,
      fontWeight: FontWeight.w300,
      height: 1.0,
      color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.57),
    );

    final maxY = data.fold(0.0, (acc, sm) {
      final value = sm.rx + sm.tx;
      return value > acc ? value : acc;
    }).toDouble();

    final horizontalInterval = getEfficientInterval(widget.height, maxY);

    final titles = <Widget>[Text("0", style: thinTextStyle)];
    for (double value = horizontalInterval; value < maxY; value += horizontalInterval) {
        titles.add(Text(toDataSize(BigInt.from(value), true), style: thinTextStyle));
    }
    final titleInterval = maxY > 0 ? (widget.height * horizontalInterval) / maxY : 0;

    return Padding(
      padding: const EdgeInsets.only(right: kPadding),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          SizedBox(
            width: kLeftAxisTitleSize,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                ...titles.mapIndexed(
                  (idx, title) =>
                      Positioned(bottom: titleInterval * idx - kFontSize / 2, right: kPadding / 2, child: title),
                ),
              ],
            ),
          ),
          ClipRect(
            clipBehavior: Clip.hardEdge,
            child: SizedBox(
              width: graphWidth,
              child: OverflowBox(
                maxWidth: graphWidth + 2 * stepWidth,
                child: Transform.translate(
                  offset: Offset(-stepWidth * (_offsetX / kUpdateIntervalMs), 0),
                  filterQuality: FilterQuality.high,
                  child: LineChart(
                    duration: Duration.zero,
                    LineChartData(
                      minY: 0,
                      maxY: maxY,
                      minX: data.first.time,
                      maxX: data.last.time,
                      lineTouchData: const LineTouchData(enabled: false),
                      clipData: const FlClipData.all(),
                      gridData: FlGridData(
                        show: true,
                        drawVerticalLine: false,
                        horizontalInterval: horizontalInterval,
                        getDrawingHorizontalLine: (value) => const FlLine(color: Colors.blueGrey, strokeWidth: 0.5),
                      ),
                      borderData: FlBorderData(
                        show: true,
                        border: Border(bottom: BorderSide(color: Colors.blueGrey, width: 0.5)),
                      ),
                      lineBarsData: [
                        LineChartBarData(
                          spots: data.map((sm) => FlSpot(sm.time, sm.tx + sm.rx)).toList(),
                          dotData: const FlDotData(show: false),
                          color: txColor,
                          barWidth: 1,
                          isCurved: true,
                        ),
                        LineChartBarData(
                          spots: data.map((sm) => FlSpot(sm.time, sm.rx)).toList(),
                          dotData: const FlDotData(show: false),
                          color: rxColor,
                          barWidth: 1,
                          isCurved: true,
                          belowBarData: BarAreaData(show: true, color: rxColor, cutOffY: 0, applyCutOffY: true),
                        ),
                      ],
                      betweenBarsData: [BetweenBarsData(fromIndex: 0, toIndex: 1, color: txColor)],
                      titlesData: FlTitlesData(show: false),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
