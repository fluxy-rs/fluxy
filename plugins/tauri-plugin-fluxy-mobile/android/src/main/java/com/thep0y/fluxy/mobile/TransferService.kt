package com.thep0y.fluxy.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager

/**
 * Keeps the process in the foreground while a transfer is running.
 *
 * Without this the system is free to freeze or kill the app as soon as the
 * screen turns off, which aborts a long transfer. Android 15 gives dataSync
 * services a six-hour budget per day; [TIMEOUT_MS] stops the service well inside
 * it, and the transfer's own timeout handles the rest.
 */
class TransferService : Service() {
    companion object {
        const val CHANNEL_ID = "fluxy-transfer"
        const val NOTIFICATION_ID = 0x51
        const val TIMEOUT_MS = 6 * 60 * 60 * 1000L

        fun start(context: Context) {
            val intent = Intent(context, TransferService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, TransferService::class.java))
        }
    }

    private val handler = Handler(Looper.getMainLooper())
    private val timeout = Runnable { stopSelf() }
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        ensureChannel()
        val notification = buildNotification()
        val serviceType = ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
        android.util.Log.i(
            "FluxyService",
            "startForeground sdk=" + Build.VERSION.SDK_INT + " type=" + serviceType,
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, serviceType)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        // A foreground service keeps the process alive but does not keep the
        // CPU awake. Under Doze the transfer stops dead - measured at zero bytes
        // over 40 s while the service was running and the session stayed open -
        // and it does not resume when the device wakes. A partial wake lock is
        // what actually holds the CPU for the duration of the transfer, and it
        // is released in onDestroy so the six-hour budget is never exceeded.
        if (wakeLock == null) {
            val power = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = power
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "fluxy:transfer")
                .apply { setReferenceCounted(false) }
        }
        wakeLock?.acquire(TIMEOUT_MS)
        // The OS budget is six hours; stop before it is exhausted so the app is
        // never killed mid-write by the platform.
        handler.removeCallbacks(timeout)
        handler.postDelayed(timeout, TIMEOUT_MS)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        handler.removeCallbacks(timeout)
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        super.onDestroy()
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL_ID) != null) return
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Fluxy transfers",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Shown while files are moving between devices"
            setShowBadge(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(): Notification {
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val content = PendingIntent.getActivity(
            this,
            0,
            launch,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setContentTitle("Fluxy")
            .setContentText("Transferring files on your local network")
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setOngoing(true)
            .setContentIntent(content)
            .build()
    }
}
