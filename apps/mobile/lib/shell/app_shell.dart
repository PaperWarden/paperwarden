import 'package:flutter/material.dart';

import '../screens/files_screen.dart';
import '../screens/home_screen.dart';
import '../screens/settings_screen.dart';
import '../screens/tools_screen.dart';
import '../theme/palette.dart';

/// The four top-level destinations, in navigation-bar order.
enum AppTab { home, files, tools, settings }

/// Bottom-navigation shell. Every tab stays mounted, so switching tabs keeps
/// scroll positions and in-progress input.
class AppShell extends StatefulWidget {
  const AppShell({super.key});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  AppTab _tab = AppTab.home;

  void _select(AppTab tab) => setState(() => _tab = tab);

  @override
  Widget build(BuildContext context) {
    return ShellScope(
      selectTab: _select,
      child: Scaffold(
        body: IndexedStack(
          index: _tab.index,
          children: const [
            HomeScreen(),
            FilesScreen(),
            ToolsScreen(),
            SettingsScreen(),
          ],
        ),
        bottomNavigationBar: DecoratedBox(
          decoration: BoxDecoration(
            border: Border(top: BorderSide(color: context.palette.outline)),
          ),
          child: NavigationBar(
            selectedIndex: _tab.index,
            onDestinationSelected: (index) => _select(AppTab.values[index]),
            destinations: const [
              NavigationDestination(
                icon: Icon(Icons.home_outlined),
                selectedIcon: Icon(Icons.home),
                label: 'Home',
              ),
              NavigationDestination(
                icon: Icon(Icons.folder_outlined),
                selectedIcon: Icon(Icons.folder),
                label: 'Files',
              ),
              NavigationDestination(
                icon: Icon(Icons.grid_view_outlined),
                selectedIcon: Icon(Icons.grid_view),
                label: 'Tools',
              ),
              NavigationDestination(
                icon: Icon(Icons.settings_outlined),
                selectedIcon: Icon(Icons.settings),
                label: 'Settings',
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Lets any widget inside the shell switch tabs, such as the settings
/// shortcut or a quick action.
class ShellScope extends InheritedWidget {
  const ShellScope({super.key, required this.selectTab, required super.child});

  final ValueChanged<AppTab> selectTab;

  /// Switches to [tab]. Safe to call from event handlers.
  static void select(BuildContext context, AppTab tab) {
    context.getInheritedWidgetOfExactType<ShellScope>()?.selectTab(tab);
  }

  @override
  bool updateShouldNotify(ShellScope oldWidget) =>
      selectTab != oldWidget.selectTab;
}
