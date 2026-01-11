import 'dart:math' as math;  
// from fl_chart /lib/src/utils
// https://github.com/imaNNeo/fl_chart/blob/main/lib/src/utils/utils.dart

/// Returns an efficient interval for showing axis titles, or grid lines or ...
///
/// If there isn't any provided interval, we use this function to calculate an interval to apply,
/// using [axisViewSize] / [pixelPerInterval], we calculate the allowedCount lines in the axis,
/// then using  [diffInAxis] / allowedCount, we can find out how much interval we need,
/// then we round that number by finding nearest number in this pattern:
/// 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 5000, 10000,...
double getEfficientInterval(
  double axisViewSize,
  double diffInAxis, {
  double pixelPerInterval = 40,
}) {
  final allowedCount = math.max(axisViewSize ~/ pixelPerInterval, 1);
  if (diffInAxis == 0) {
    return 1;
  }
  final accurateInterval =
      diffInAxis == 0 ? axisViewSize : diffInAxis / allowedCount;
  if (allowedCount <= 2) {
    return accurateInterval;
  }
  return roundInterval(accurateInterval);
}

double roundInterval(double input) {
  if (input < 1) {
    return _roundIntervalBelowOne(input);
  }
  return _roundIntervalAboveOne(input);
}

double _roundIntervalBelowOne(double input) {
  assert(input < 1.0);

  if (input < 0.000001) {
    return input;
  }

  final inputString = input.toString();
  var precisionCount = inputString.length - 2;

  var zeroCount = 0;
  for (var i = 2; i <= inputString.length; i++) {
    if (inputString[i] != '0') {
      break;
    }
    zeroCount++;
  }

  final afterZerosNumberLength = precisionCount - zeroCount;
  if (afterZerosNumberLength > 2) {
    final numbersToRemove = afterZerosNumberLength - 2;
    precisionCount -= numbersToRemove;
  }

  final pow10onPrecision = math.pow(10, precisionCount);
  input *= pow10onPrecision;
  return _roundIntervalAboveOne(input) / pow10onPrecision;
}

double _roundIntervalAboveOne(double input) {
  assert(input >= 1.0);
  final decimalCount = input.toInt().toString().length - 1;
  input /= math.pow(10, decimalCount);

  final scaled = input >= 10 ? input.round() / 10 : input;

  if (scaled >= 7.6) {
    return 10 * math.pow(10, decimalCount).toInt().toDouble();
  } else if (scaled >= 2.6) {
    return 5 * math.pow(10, decimalCount).toInt().toDouble();
  } else if (scaled >= 1.6) {
    return 2 * math.pow(10, decimalCount).toInt().toDouble();
  } else {
    return 1 * math.pow(10, decimalCount).toInt().toDouble();
  }
}
