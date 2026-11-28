package dev.paperwarden.paperwarden

import android.Manifest
import android.app.Service
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.Parcel
import android.os.Process
import java.io.File

/**
 * Hosts PDFium in its own disposable process (#59, ADR-0007).
 *
 * Declared with `android:isolatedProcess`: the process runs under a fresh
 * user id with no permissions, no network and no access to the app's files.
 * It receives the staged input and the job's output as file descriptors and
 * hands them to Rust (`paperwarden_pdf::service::serve`), which confines the
 * process (time limit, memory ceiling) before PDFium is loaded. This class is
 * transport only; it never reads the document.
 *
 * One process serves one job: [RenderClient] unbinds after each job, and
 * Android then discards the process.
 */
class RenderService : Service() {
    private val binder = object : Binder() {
        override fun onTransact(code: Int, data: Parcel, reply: Parcel?, flags: Int): Boolean {
            if (code != RenderProtocol.RENDER) return super.onTransact(code, data, reply, flags)
            data.enforceInterface(RenderProtocol.DESCRIPTOR)
            // Only PaperWarden itself may ask; the service is not exported.
            if (Binder.getCallingUid() != applicationInfo.uid) {
                throw SecurityException("render requests come from PaperWarden only")
            }
            val input = data.readFileDescriptor()
            val output = data.readFileDescriptor()
            val maxWidth = data.readInt()
            val maxHeight = data.readInt()
            val maxInputBytes = data.readLong()
            val maxOutputBytes = data.readLong()
            val timeLimitMs = data.readLong()
            val memoryBudgetBytes = data.readLong()
            val status = if (input == null || output == null) {
                input?.close()
                output?.close()
                RenderProtocol.INVALID_REQUEST
            } else {
                // Ownership of both descriptors passes to Rust, which closes them.
                RenderNative.render(
                    input.detachFd(), output.detachFd(), maxWidth, maxHeight,
                    maxInputBytes, maxOutputBytes, timeLimitMs, memoryBudgetBytes,
                )
            }
            reply?.writeInt(status)
            reply?.writeInt(Process.myPid())
            reply?.writeInt(Process.myUid())
            reply?.writeInt(if (isIsolated()) 1 else 0)
            reply?.writeInt(if (internetGranted()) 1 else 0)
            reply?.writeStringList(apkMappings())
            return true
        }
    }

    override fun onBind(intent: Intent): IBinder = binder

    private fun isIsolated(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && Process.isIsolated()

    // Evidence for the device tests: which parts of the APK this process has
    // mapped. Native libraries load straight from the APK, so a mapping's file
    // offset shows which library it is (PaperWarden's PDFium lives here only).
    private fun apkMappings(): List<String> =
        runCatching {
            File("/proc/self/maps").readLines().filter { it.endsWith(applicationInfo.sourceDir) }
        }.getOrDefault(emptyList())

    // Even a debug build, whose app process may hold INTERNET for Flutter's
    // tooling, gives the isolated process no network permission.
    private fun internetGranted(): Boolean =
        checkSelfPermission(Manifest.permission.INTERNET) == PackageManager.PERMISSION_GRANTED
}

/** The Binder contract between [RenderClient] and [RenderService]. */
internal object RenderProtocol {
    const val DESCRIPTOR = "dev.paperwarden.paperwarden.RenderService"
    const val RENDER = IBinder.FIRST_CALL_TRANSACTION

    /** `ServiceStatus::InvalidRequest` (core/crates/pdf/src/service.rs). */
    const val INVALID_REQUEST = 1

    /** `ServiceStatus::AlreadyUsed`. */
    const val ALREADY_USED = 2
}

/** The JNI entry point in the Rust bridge library (`service_jni.rs`). */
internal object RenderNative {
    init {
        System.loadLibrary("paperwarden_bridge")
    }

    @JvmStatic
    external fun render(
        input: Int,
        output: Int,
        maxWidth: Int,
        maxHeight: Int,
        maxInputBytes: Long,
        maxOutputBytes: Long,
        timeLimitMs: Long,
        memoryBudgetBytes: Long,
    ): Int
}
