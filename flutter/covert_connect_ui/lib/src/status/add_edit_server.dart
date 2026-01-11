import 'dart:developer';
import 'dart:math' as math;

import 'package:covert_connect/di.dart';
import 'package:covert_connect/src/rust/api/service.dart';
import 'package:covert_connect/src/rust/api/wrappers.dart';
import 'package:covert_connect/src/services/proxy_service.dart';
import 'package:covert_connect/src/utils/exception_helper.dart';
import 'package:covert_connect/src/utils/svg.dart';
import 'package:covert_connect/src/utils/uri.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/app_theme.dart';
import 'package:covert_connect/src/widgets/button.dart';
import 'package:covert_connect/src/widgets/toast.dart';
import 'package:covert_connect/src/widgets/input.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:shimmer/shimmer.dart';
import 'package:vector_math/vector_math_64.dart' as vector_math;

class AddEditServerPage extends StatefulWidget {
  const AddEditServerPage({super.key, this.server});

  final ServerInfo? server;

  @override
  State<AddEditServerPage> createState() => _AddEditServerPageState();
}

class _AddEditServerPageState extends State<AddEditServerPage> {
  ServerInfo? get server => widget.server;

  ProtocolConfig? _protocol;
  bool _waitingProtocol = false;

  final TextEditingController _hostController = TextEditingController();
  final TextEditingController _keyController = TextEditingController();
  final TextEditingController _nameController = TextEditingController();

  String _keyError = "";
  String _hostError = "";

  void _onChangedConnection() async {
    _hostError = "";
    _keyError = "";

    final key = _keyController.text;
    final host = _hostController.text;
    if (host.isEmpty || key.isEmpty) return;

    if (key.length != 43) {
      _protocol = null;
      _keyError = "access key should be 43 symbols long";
      _updateIfMounted();
      return;
    }

    _waitingProtocol = true;
    try {
      _protocol = await di<ProxyServiceBase>().getServerProtocol(host, key);
    } catch (e) {
      final errMsg = exceptionToString(e);
      di<ProxyServiceBase>().log(errMsg);

      final errMsgLower = errMsg.toLowerCase();
      if (["connection", "host", "peer"].any((word) => errMsgLower.contains(word))) {
        _hostError = "Host is unreachable";
      } else if (["header size", "decrypt header"].any((word) => errMsgLower.contains(word))) {
        _keyError = "Invalid key or wrong host";
      } else {
        _keyError = errMsg;
      }

      // TODO: ??? try reconnect after some delay
    } finally {
      _waitingProtocol = false;
    }

    _updateIfMounted();
  }

  void _save() async {
    if (_protocol == null) return;

    final newConfig = ServerConfig(
      host: _hostController.text,
      caption: _nameController.text,
      domains: widget.server?.config.domains,
      enabled: widget.server?.config.enabled ?? true,
      protocol: _protocol!,
    );

    try {
      if (widget.server != null) {
        await di<ProxyServiceBase>().updateServer(widget.server!.config.host, newConfig);
      } else {
        await di<ProxyServiceBase>().addServer(newConfig);
      }
      _back();
    } catch (e) {
      log("error: $e");
      if (mounted) Toast.error(context, caption: "Save Server Error", text: e.toString());
    }
  }

  void _delete() async {
    if (server == null) return;
    try {
      await di<ProxyServiceBase>().deleteServer(server!.config.host);
      if (mounted) {
        Toast.success(
          context,
          caption: server!.config.host,
          text: "was removed, ${isDesktop ? 'click to undo' : 'tap to undo'}",
          onTap: () => di<ProxyServiceBase>().addServer(server!.config),
        );
      }
      _back();
    } catch (e) {
      log("error: $e");
      if (mounted) Toast.error(context, caption: "Delete Server Error", text: e.toString());
    }
  }

  void _back() {
    Navigator.of(context).pop();
  }

  Future<void> _tryGetFromClipboard() async {
    try {
      ClipboardData? data = await Clipboard.getData(Clipboard.kTextPlain);
      if (data?.text?.isEmpty ?? true) {
        return;
      }

      final uriData = data!.text!.tryParseUri();
      if (uriData != null) {
        _hostController.text = uriData.host;
        _keyController.text = uriData.key;
        _nameController.text = uriData.name ?? "";
      }
    } finally {
      _onChangedConnection();
    }
  }

  void _updateIfMounted() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    if (server != null) {
      _hostController.text = server!.config.host;
      _keyController.text = server!.config.protocol.key;
      _nameController.text = server!.config.caption ?? "";
      _onChangedConnection();
    } else {
      _tryGetFromClipboard();
    }
    super.initState();
  }

  @override
  void dispose() {
    _hostController.dispose();
    _keyController.dispose();
    _nameController.dispose();
    super.dispose();
  }

  Widget buildProtocolData() {
    if (_protocol == null) {
      return Container();
    }

    return Column(
      children: [
        _ProtocolItem("Cipher", _protocol!.cipher.toString()),
        _ProtocolItem("Key derivation algorithm", _protocol!.kdf.toString()),
        _ProtocolItem("Max connection delay", "${_protocol!.maxConnectDelay}ms"),
        _ProtocolItem("Data padding", "${_protocol!.dataPadding.rate}% of payload"),
        _ProtocolItem("Data padding maximum length", "${_protocol!.dataPadding.max} bytes"),
        _ProtocolItem("Header padding", "${_protocol!.headerPadding.start}..${_protocol!.headerPadding.end}"),
        _ProtocolItem("Encription limit per connection", _protocol!.encryptionLimit.toString()),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final dark = theme.brightness == Brightness.dark;

    final protocolWidget = _Expandable(caption: "Protocol Settings", body: buildProtocolData());
    return Scaffold(
      backgroundColor: context.appColors.scaffoldBackgroundSecondary,
      body: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Column(
          spacing: 12,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Text(
                server == null ? "Add new server" : "Edit server",
                style: theme.textTheme.bodyLarge?.copyWith(
                  color: theme.textTheme.bodyLarge?.color?.withValues(alpha: 0.9),
                ),
              ),
            ),
            _Input(caption: "Host", controller: _hostController, error: _hostError, onChanged: _onChangedConnection),
            _Input(
              caption: "Access Key",
              controller: _keyController,
              error: _keyError,
              borderColor: _protocol != null ? Colors.greenAccent.withValues(alpha: 0.7) : null,
              onChanged: _onChangedConnection,
            ),
            _Input(caption: "Name", controller: _nameController),
            if (_protocol != null) protocolWidget,
            if (_protocol == null && _waitingProtocol)
              Shimmer.fromColors(
                enabled: _waitingProtocol,
                baseColor: theme.colorScheme.onSurface.withValues(alpha: dark ? 0.3 : 0.7),
                highlightColor: dark ? theme.colorScheme.onSurface : Colors.white,
                child: protocolWidget,
              ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Button(label: "Cancel", onTap: _back),
                Row(
                  spacing: 8,
                  children: [
                    if (server != null) Button(label: "Delete", onTap: _delete),
                    Button(label: "Save", onTap: _protocol != null ? _save : null, primary: true),
                  ],
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _ProtocolItem extends StatelessWidget {
  const _ProtocolItem(this.name, this.value);

  final String name;
  final String value;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final valueStyle = GoogleFonts.inter(
      fontSize: 14,
      fontWeight: FontWeight.w200,
      height: 1.0,
      color: colorScheme.onSurface.withValues(alpha: 0.57),
    );
    return Row(
      children: [
        Text("$name: "),
        Expanded(
          child: Text(value, style: valueStyle.copyWith(overflow: TextOverflow.ellipsis)),
        ),
      ],
    );
  }
}

class _Input extends StatelessWidget {
  const _Input({required this.caption, required this.controller, this.error, this.borderColor, this.onChanged});

  final String caption;
  final VoidCallback? onChanged;
  final TextEditingController controller;
  final String? error;
  final Color? borderColor;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Column(
          spacing: 3,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(caption),
            Input(
              error: error?.isNotEmpty ?? false,
              borderColor: borderColor,
              controller: controller,
              onChanged: (_) => onChanged?.call(),
              keyboardType: TextInputType.text,
              padding: EdgeInsets.symmetric(horizontal: 8, vertical: 3),
            ),
            SizedBox(height: 2),
          ],
        ),
        if (error?.isNotEmpty ?? false)
          Positioned(
            left: 9,
            bottom: -10,
            child: Text(
              error!,
              style: textTheme.bodySmall?.copyWith(fontSize: 11, color: Colors.redAccent.withValues(alpha: 0.67)),
            ),
          ),
      ],
    );
  }
}

class _Expandable extends StatefulWidget {
  const _Expandable({required this.caption, required this.body});

  final String caption;
  final Widget body;

  @override
  State<_Expandable> createState() => _ExpandableState();
}

class _ExpandableState extends State<_Expandable> with SingleTickerProviderStateMixin {
  static const _animationDuration = Durations.medium2;
  late final AnimationController _animation = AnimationController(vsync: this, duration: _animationDuration);

  bool _expanded = false;

  static final Matrix4 _pmat = Matrix4(
    // dart format off
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 1.0 * 0.001,
    0.0, 0.0, 0.0, 1.0,
    // dart format on
  );

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () {
        setState(() {
          _expanded = !_expanded;
          if (_expanded) {
            _animation.forward();
          } else {
            _animation.reverse();
          }
        });
      },
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  widget.caption,
                  style: TextStyle(fontWeight: FontWeight.bold, color: colorScheme.onSurface.withValues(alpha: 0.8)),
                ),
              ),
              AnimatedBuilder(
                animation: _animation,
                builder: (_, child) => Container(
                  margin: EdgeInsets.only(top: 4),
                  width: 12,
                  height: 10,
                  child: Transform(
                    alignment: FractionalOffset(0.5, 0.6),
                    transform: _pmat.scaledByVector3(vector_math.Vector3(1.0, 1.0, 1.0))
                      ..rotateX(math.pi - ((1.0 - _animation.value) * 180) * math.pi / 180)
                      ..rotateY(0.0)
                      ..rotateZ(0.0),
                    child: buildSvg("assets/icons/chevron-down.svg", color: colorScheme.onSurface),
                  ),
                ),
              ),
              SizedBox(width: 8),
            ],
          ),
          SizedBox(height: 6),
          AnimatedCrossFade(
            firstChild: SizedBox(height: 1, width: double.infinity),
            secondChild: widget.body,
            firstCurve: const Interval(0.0, 0.6, curve: Curves.fastOutSlowIn),
            secondCurve: const Interval(0.4, 1.0, curve: Curves.fastOutSlowIn),
            sizeCurve: Curves.fastOutSlowIn,
            crossFadeState: _expanded ? CrossFadeState.showSecond : CrossFadeState.showFirst,
            duration: _animationDuration,
          ),
        ],
      ),
    );
  }
}
