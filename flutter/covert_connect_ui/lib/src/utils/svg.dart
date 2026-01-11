import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class AnyColorMapper extends ColorMapper {
  const AnyColorMapper(this._color);

  final Color _color;

  @override
  Color substitute(String? id, String elementName, String attributeName, Color color) {
    return _color;
  }
}

Widget buildSvg(String url, {Color? color, double? width, double? height}) {
  return SvgPicture.asset(url, colorMapper: color != null ? AnyColorMapper(color) : null, width: width, height: height);
}
