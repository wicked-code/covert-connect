import 'package:flutter/material.dart';

class HoverBuilder extends StatefulWidget {
  const HoverBuilder({super.key, required this.builder});

  final Widget Function(BuildContext context, bool isHovering) builder;

  @override
  State<HoverBuilder> createState() => _HoverBuilderState();
}

class _HoverBuilderState extends State<HoverBuilder> {
  bool _hovering = false;
  @override
  Widget build(BuildContext context) {
    return Hover(
      onChange: (hovering) {
        setState(() {
          _hovering = hovering;
        });
      },
      child: widget.builder(context, _hovering),
    );
  }
}

class Hover extends StatefulWidget {
  const Hover({super.key, required this.onChange, this.child});

  final ValueChanged<bool> onChange;
  final Widget? child;

  @override
  State<Hover> createState() => _HoverState();
}

class _HoverState extends State<Hover> {
  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (event) {
        widget.onChange(true);
      },
      onExit: (event) {
        widget.onChange(false);
      },
      child: widget.child,
    );
  }
}
