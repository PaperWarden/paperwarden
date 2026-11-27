import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "PaperWardenStorage") {
      let channel = FlutterMethodChannel(
        name: "dev.paperwarden/storage", binaryMessenger: registrar.messenger())
      channel.setMethodCallHandler { call, result in
        guard call.method == "databaseDirectory" else {
          result(FlutterMethodNotImplemented)
          return
        }
        do {
          result(try AppDelegate.databaseDirectory())
        } catch {
          result(FlutterError(code: "unavailable", message: "storage directory unavailable", details: nil))
        }
      }
    }
  }

  /// Thin adapter: tells the core where its database lives (ADR-0006). The
  /// directory is excluded from iCloud and device backups, and its files are
  /// encrypted while the device is locked unless already open, so SQLite can
  /// keep an open database working in the background.
  private static func databaseDirectory() throws -> String {
    let support = try FileManager.default.url(
      for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    var directory = support.appendingPathComponent("PaperWarden", isDirectory: true)
    try FileManager.default.createDirectory(
      at: directory, withIntermediateDirectories: true,
      attributes: [.protectionKey: FileProtectionType.completeUnlessOpen])
    try FileManager.default.setAttributes(
      [.protectionKey: FileProtectionType.completeUnlessOpen], ofItemAtPath: directory.path)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try directory.setResourceValues(values)
    return directory.path
  }
}
