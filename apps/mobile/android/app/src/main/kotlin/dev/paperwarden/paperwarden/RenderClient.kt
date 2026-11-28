package dev.paperwarden.paperwarden

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.DeadObjectException
import android.os.IBinder
import android.os.Parcel
import android.os.ParcelFileDescriptor
import android.os.Process
import android.os.SystemClock
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import java.util.concurrent.atomic.AtomicReference

/**
 * Runs one render in a fresh [RenderService] process and waits for it (#59).
 *
 * The app process never loads PDFium. For each job this binds the isolated
 * service, passes the staged input and the job's output as file descriptors,
 * and waits at most the job's time limit plus [GRACE_MS]. Then it unbinds,
 * which makes Android discard the process, and waits for the process to die,
 * so the next job always starts in a new one. A render that overruns is
 * killed that way; a process that crashes or hits its memory ceiling only
 * ends the job.
 *
 * Paths are those the Rust job runner chose inside the job's workspace;
 * nothing here reads or writes the document.
 */
class RenderClient(private val context: Context) {
    data class Request(
        val input: String,
        val output: String,
        val maxWidth: Int,
        val maxHeight: Int,
        val maxInputBytes: Long,
        val maxOutputBytes: Long,
        val timeLimitMs: Long,
        val memoryBudgetBytes: Long,
    )

    fun render(request: Request): Map<String, Any> {
        val connected = CountDownLatch(1)
        val service = AtomicReference<IBinder?>()
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, binder: IBinder) {
                service.set(binder)
                connected.countDown()
            }

            override fun onServiceDisconnected(name: ComponentName) {}
        }
        val started = SystemClock.elapsedRealtime()
        val intent = Intent(context, RenderService::class.java)
        if (!context.bindService(intent, connection, Context.BIND_AUTO_CREATE)) {
            context.unbindService(connection)
            return ended("unavailable")
        }
        val died = CountDownLatch(1)
        try {
            if (!connected.await(START_TIMEOUT_MS, TimeUnit.MILLISECONDS)) return ended("unavailable")
            val binder = service.get() ?: return ended("unavailable")
            binder.linkToDeath({ died.countDown() }, 0)
            val call = Executors.newSingleThreadExecutor()
            try {
                val answer = call.submit<Map<String, Any>> { transact(binder, request) }
                val wait = request.timeLimitMs + GRACE_MS
                return try {
                    answer.get(wait, TimeUnit.MILLISECONDS)
                } catch (_: TimeoutException) {
                    ended("timedOut")
                } catch (error: ExecutionException) {
                    // The process died mid-call: the watchdog at the time
                    // limit, PDFium crashing, or the memory ceiling.
                    val elapsed = SystemClock.elapsedRealtime() - started
                    if (error.cause is DeadObjectException && elapsed >= request.timeLimitMs) {
                        ended("timedOut")
                    } else {
                        ended("died")
                    }
                }
            } finally {
                call.shutdownNow()
            }
        } finally {
            context.unbindService(connection)
            // An isolated service with no bindings left is killed; wait so the
            // next job never reuses this process.
            if (service.get() != null) died.await(DEATH_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        }
    }

    private fun transact(binder: IBinder, request: Request): Map<String, Any> {
        val input = ParcelFileDescriptor.open(File(request.input), ParcelFileDescriptor.MODE_READ_ONLY)
        val output = ParcelFileDescriptor.open(
            File(request.output),
            ParcelFileDescriptor.MODE_WRITE_ONLY or ParcelFileDescriptor.MODE_TRUNCATE,
        )
        val data = Parcel.obtain()
        val reply = Parcel.obtain()
        try {
            data.writeInterfaceToken(RenderProtocol.DESCRIPTOR)
            data.writeFileDescriptor(input.fileDescriptor)
            data.writeFileDescriptor(output.fileDescriptor)
            data.writeInt(request.maxWidth)
            data.writeInt(request.maxHeight)
            data.writeLong(request.maxInputBytes)
            data.writeLong(request.maxOutputBytes)
            data.writeLong(request.timeLimitMs)
            data.writeLong(request.memoryBudgetBytes)
            // Our copies close below; the service holds its own duplicates.
            binder.transact(RenderProtocol.RENDER, data, reply, 0)
            return mapOf(
                "end" to "finished",
                "status" to reply.readInt(),
                "pid" to reply.readInt(),
                "uid" to reply.readInt(),
                "isolated" to (reply.readInt() == 1),
                "internetGranted" to (reply.readInt() == 1),
                "apkMappings" to (reply.createStringArrayList() ?: arrayListOf<String>()),
                "appPid" to Process.myPid(),
                "appUid" to Process.myUid(),
            )
        } finally {
            data.recycle()
            reply.recycle()
            input.close()
            output.close()
        }
    }

    private fun ended(end: String): Map<String, Any> = mapOf(
        "end" to end,
        "status" to -1,
        "appPid" to Process.myPid(),
        "appUid" to Process.myUid(),
    )

    companion object {
        const val GRACE_MS = 2_000L
        const val START_TIMEOUT_MS = 10_000L
        const val DEATH_TIMEOUT_MS = 2_000L
    }
}
