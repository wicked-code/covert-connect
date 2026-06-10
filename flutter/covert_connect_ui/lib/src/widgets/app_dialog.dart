import 'package:flutter/material.dart';

class AppDialog extends StatelessWidget {
  const AppDialog({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final borderRadius = BorderRadius.circular(8);
    final colorScheme = Theme.of(context).colorScheme;
    return Center(
      child: Container(
        margin: EdgeInsets.all(16),
        padding: EdgeInsets.all(16),
        decoration: BoxDecoration(color: colorScheme.surface.withValues(alpha: 1.0), borderRadius: borderRadius),
        child: child,
      ),
    );
  }
}
