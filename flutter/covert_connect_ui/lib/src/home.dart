import 'package:covert_connect/src/domains/domains.dart';
import 'package:covert_connect/src/log.dart';
import 'package:covert_connect/src/options.dart';
import 'package:covert_connect/src/status/status.dart';
import 'package:covert_connect/src/utils/child_router.dart';
import 'package:covert_connect/src/utils/utils.dart';
import 'package:covert_connect/src/widgets/desktop/desktop_app_bar.dart';
import 'package:covert_connect/src/widgets/mobile/mobile_footer.dart';
import 'package:flutter/material.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> with SingleTickerProviderStateMixin {
  static GlobalKey<ChildNavigatorState> statusPageNavigatorKey = GlobalKey<ChildNavigatorState>();
  late TabController _tabController;

  void _onTabChanged() {
    setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 4, vsync: this);
    _tabController.addListener(_onTabChanged);
  }

  @override
  void dispose() {
    _tabController.removeListener(_onTabChanged);
    _tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          if (isDesktop) DesktopAppBar(tabController: _tabController),
          Expanded(
            child: TabBarView(
              controller: _tabController,
              children: [
                ChildNavigator(
                  key: statusPageNavigatorKey,
                  selected: _tabController.index == 0,
                  builder: (context) => StatusPage(),
                ),
                DomainsPage(),
                LogPage(),
                OptionsPage(),
              ],
            ),
          ),
          if (!isDesktop) MobileFooter(tabController: _tabController),
        ],
      ),
    );
  }
}
