package dev.paperwarden.paperwarden

import android.os.Handler
import android.os.Looper
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.Executors

class MainActivity : FlutterActivity() {
    // One render at a time: each job gets its own service process (#59).
    private val renders = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.paperwarden/render")
            .setMethodCallHandler { call, result ->
                if (call.method != "render") return@setMethodCallHandler result.notImplemented()
                val request = parse(call)
                    ?: return@setMethodCallHandler result.error("invalid", "missing argument", null)
                val client = RenderClient(applicationContext)
                renders.execute {
                    val answer = client.render(request)
                    main.post { result.success(answer) }
                }
            }
    }

    override fun onDestroy() {
        renders.shutdown()
        super.onDestroy()
    }

    private fun parse(call: MethodCall): RenderClient.Request? {
        fun long(name: String): Long? = (call.argument<Any>(name) as? Number)?.toLong()
        fun int(name: String): Int? = (call.argument<Any>(name) as? Number)?.toInt()
        return RenderClient.Request(
            input = call.argument<String>("input") ?: return null,
            output = call.argument<String>("output") ?: return null,
            maxWidth = int("maxWidth") ?: return null,
            maxHeight = int("maxHeight") ?: return null,
            maxInputBytes = long("maxInputBytes") ?: return null,
            maxOutputBytes = long("maxOutputBytes") ?: return null,
            timeLimitMs = long("timeLimitMs") ?: return null,
            memoryBudgetBytes = long("memoryBudgetBytes") ?: return null,
        )
    }
}
