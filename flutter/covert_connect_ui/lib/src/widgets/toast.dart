import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/widgets/app_icon_button.dart';
import 'package:flutter/material.dart';
import 'package:toastification/toastification.dart';
import 'package:iconsax_flutter/iconsax_flutter.dart';

enum ToastType { success, error, warning }

const erorColor = Color(0xFFFF3615);
const warningColor = Color(0xFFFFB600);
const successColor = Color(0xFF22FF65);

class Toast extends StatelessWidget {
  const Toast({
    super.key,
    required this.item,
    required this.type,
    required this.caption,
    required this.text,
    this.onTap,
  });

  static void error(
    BuildContext context, {
    required String caption,
    required String text,
    Duration? autoCloseDuration,
    VoidCallback? onTap,
  }) {
    show(
      context,
      type: ToastType.error,
      caption: caption,
      text: text,
      autoCloseDuration: autoCloseDuration,
      onTap: onTap,
    );
  }

  static void warning(
    BuildContext context, {
    required String caption,
    required String text,
    Duration? autoCloseDuration,
    VoidCallback? onTap,
  }) {
    show(
      context,
      type: ToastType.warning,
      caption: caption,
      text: text,
      autoCloseDuration: autoCloseDuration,
      onTap: onTap,
    );
  }

  static void success(
    BuildContext context, {
    required String caption,
    required String text,
    Duration? autoCloseDuration,
    VoidCallback? onTap,
  }) {
    show(
      context,
      type: ToastType.success,
      caption: caption,
      text: text,
      autoCloseDuration: autoCloseDuration,
      onTap: onTap,
    );
  }

  static void show(
    BuildContext context, {
    required ToastType type,
    required String caption,
    required String text,
    Duration? autoCloseDuration,
    VoidCallback? onTap,
  }) {
    toastification.showCustom(
      context: context,
      alignment: Alignment.bottomRight,
      autoCloseDuration: autoCloseDuration == Duration.zero ? null : autoCloseDuration ?? const Duration(seconds: 5),
      builder: (context, holder) {
        return Toast(item: holder, type: type, caption: caption, text: text, onTap: onTap);
      },
    );
  }

  final ToastificationItem item;
  final String caption;
  final String text;
  final ToastType type;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final textTheme = Theme.of(context).textTheme;
    final color = switch (type) {
      ToastType.error => errorColor,
      ToastType.warning => warningColor,
      ToastType.success => successColor,
    };
    final icon = switch (type) {
      ToastType.error || ToastType.warning => Iconsax.danger_copy,
      ToastType.success => Iconsax.copy_success,
    };

    return Dismissible(
      key: UniqueKey(),
      onDismissed: (_) {
        final notification = toastification.findToastificationItem(item.id);
        if (notification != null) {
          toastification.dismiss(notification, showRemoveAnimation: false);
        }
      },
      dismissThresholds: {DismissDirection.horizontal: 0.9},
      child: MouseRegion(
        onEnter: (event) {
          item.pause();
        },
        onExit: (event) {
          item.start();
        },
        child: GestureDetector(
          onTap: () async {
            onTap?.call();
            toastification.dismissById(item.id);
          },
          child: Container(
            margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            padding: const EdgeInsets.only(left: 12, right: 8, top: 8, bottom: 8),
            decoration: BoxDecoration(
              color: colorScheme.surface,
              border: Border(left: BorderSide(color: color, width: 2)),
              boxShadow: [BoxShadow(color: Colors.black.withValues(alpha: 0.17), blurRadius: 20)],
            ),
            child: Row(
              spacing: 12,
              children: [
                Icon(icon, color: color),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        caption,
                        style: textTheme.titleSmall?.copyWith(
                          fontWeight: FontWeight.bold,
                          fontStyle: FontStyle.italic,
                          height: 1.2,
                        ),
                      ),
                      Text(text, style: textTheme.bodyMedium),
                      SizedBox(height: 4),
                      if (item.autoCloseDuration != null)
                        ToastTimerAnimationBuilder(
                          item: item,
                          builder: (context, value, _) {
                            return LinearProgressIndicator(
                              value: value,
                              minHeight: 2,
                              backgroundColor: Colors.transparent,
                              color: color.withValues(alpha: 0.67),
                            );
                          },
                        ),
                    ],
                  ),
                ),
                AppIconButton(
                  icon: buildSvg("assets/icons/windows-x.svg", color: colorScheme.onSurface),
                  onPressed: () {
                    toastification.dismissById(item.id);
                  },
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
