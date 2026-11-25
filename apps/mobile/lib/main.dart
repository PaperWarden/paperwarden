import 'package:flutter/material.dart';

void main() => runApp(const PaperWardenApp());

class PaperWardenApp extends StatelessWidget {
  const PaperWardenApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'PaperWarden',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF315C4B),
        ),
        useMaterial3: true,
      ),
      home: const Scaffold(
        body: SafeArea(
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.description_outlined, size: 64),
                SizedBox(height: 16),
                Text('PaperWarden', style: TextStyle(fontSize: 28)),
                SizedBox(height: 8),
                Text('Private document tools. On your device.'),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
