package dev.paperwarden.paperwarden

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // Thin adapter: tells the core where its database lives. The app's
        // files directory is private, credential-encrypted on Android 10+,
        // and excluded from backup and device transfer (res/xml rules).
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, STORAGE_CHANNEL)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "databaseDirectory" -> {
                        val directory = File(filesDir, "paperwarden")
                        if (directory.isDirectory || directory.mkdirs()) {
                            result.success(directory.absolutePath)
                        } else {
                            result.error("unavailable", "storage directory unavailable", null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private companion object {
        const val STORAGE_CHANNEL = "dev.paperwarden/storage"
    }
}
