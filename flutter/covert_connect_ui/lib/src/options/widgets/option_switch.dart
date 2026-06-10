import 'package:flutter/material.dart';

class OptionSwitch extends StatefulWidget {
  const OptionSwitch({super.key, required this.value, required this.onToggle, this.disabled = false});

  final bool value;
  final ValueChanged<bool> onToggle;
  final bool disabled;

  @override
  State<OptionSwitch> createState() => _OptionSwitchState();
}

class _OptionSwitchState extends State<OptionSwitch> with SingleTickerProviderStateMixin {
  late final Animation _toggleAnimation;
  late final Animation<double> _colorAnimation;
  late final AnimationController _animationController;

  @override
  void initState() {
    super.initState();
    _animationController = AnimationController(
      vsync: this,
      value: widget.value ? 1.0 : 0.0,
      duration: Durations.short4,
    );
    _colorAnimation = CurvedAnimation(parent: _animationController, curve: Curves.linear);
    _toggleAnimation = AlignmentTween(begin: Alignment.centerLeft, end: Alignment.centerRight).animate(_colorAnimation);
  }

  @override
  void dispose() {
    _animationController.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(OptionSwitch oldWidget) {
    super.didUpdateWidget(oldWidget);

    if (oldWidget.value == widget.value) return;

    if (widget.value) {
      _animationController.forward();
    } else {
      _animationController.reverse();
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    final activeColor = colorScheme.secondary;
    final activeToggleColor = colorScheme.onSecondary;
    final inactiveToggleColor = colorScheme.onSecondary;
    final inactiveColor = theme.brightness == Brightness.dark
        ? colorScheme.outline
        : colorScheme.outline.withValues(alpha: 0.33);

    return AnimatedBuilder(
      animation: _animationController,
      builder: (context, child) {
        final colorAnimationValue = 1 - _colorAnimation.value;
        return Align(
          child: GestureDetector(
            onTap: () {
              if (!widget.disabled) {
                if (widget.value) {
                  _animationController.forward();
                } else {
                  _animationController.reverse();
                }

                widget.onToggle(!widget.value);
              }
            },
            child: Opacity(
              opacity: widget.disabled ? 0.7 : 1,
              child: Container(
                width: 48,
                height: 25,
                padding: EdgeInsets.all(4.0),
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(15),
                  color: Color.lerp(activeColor, inactiveColor, colorAnimationValue),
                ),
                child: Stack(
                  children: <Widget>[
                    Align(
                      alignment: _toggleAnimation.value,
                      child: Container(
                        width: 20.0,
                        height: 20.0,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: Color.lerp(activeToggleColor, inactiveToggleColor, colorAnimationValue),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
