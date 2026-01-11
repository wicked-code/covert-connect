import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/status/widgets/traffic_graph.dart';
import 'package:covert_connect/src/utils/text_utils.dart';
import 'package:flutter/material.dart';

class TrafficCell extends StatelessWidget {
  const TrafficCell({
    super.key,
    required this.state,
    required this.rxChanged,
    required this.txChanged,
    required this.style,
    required this.subStyle,
  });

  final ServerState state;
  final BigInt rxChanged;
  final BigInt txChanged;
  final TextStyle style;
  final TextStyle subStyle;

  @override
  Widget build(BuildContext context) {
    final showNothing = rxChanged == BigInt.zero && txChanged == BigInt.zero;
    final showRxSpeed = !showNothing && rxChanged >= txChanged;
    final showTxSpeed = !showNothing && rxChanged < txChanged;
    final msPassed = BigInt.from(kUpdateInterval.inMilliseconds);
    final msInSec = BigInt.from(1000);
    return Padding(
      padding: EdgeInsets.only(top: 6, bottom: 6),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (showNothing) Text("-", style: style),
              if (showRxSpeed) Text(toDataSize((rxChanged * msInSec) ~/ msPassed, true), style: style),
              if (showRxSpeed) Text("↓", style: style.merge(TextStyle(color: Colors.green))),
              if (showTxSpeed) Text(toDataSize((txChanged * msInSec) ~/ msPassed, true), style: style),
              if (showTxSpeed) Text("↑", style: style.merge(TextStyle(color: Colors.red))),
            ],
          ),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(toDataSize(state.rxTotal), style: subStyle),
              Text("↓", style: rxChanged > BigInt.zero ? subStyle.merge(TextStyle(color: Colors.green)) : subStyle),
              Text(toDataSize(state.txTotal), style: subStyle),
              Text("↑", style: rxChanged > BigInt.zero ? subStyle.merge(TextStyle(color: Colors.red)) : subStyle),
            ],
          ),
        ],
      ),
    );
  }
}
