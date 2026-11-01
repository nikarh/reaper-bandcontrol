package dev.nikarh.bandcontrol.app

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Keeps the app running while it is connected and in the background (reading a message, the screen locked), so the
 * session survives instead of being frozen by Android and dropped. The connection itself lives in [AppModel]; this
 * service only holds the process in the foreground and shows the notification with a Disconnect action.
 */
class ConnectionService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        if (intent?.action == ACTION_DISCONNECT) {
            KeepAlive.disconnect?.invoke()
            stop(this)
            return START_NOT_STICKY
        }
        val text = intent?.getStringExtra(EXTRA_TEXT).orEmpty()
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification(this, text),
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0,
            )
        } catch (e: RuntimeException) {
            // E.g. without the Bluetooth permission (development connections over TCP): the app then just doesn't
            // stay connected in the background.
            Log.w(TAG, "can't keep the connection in the background", e)
            stopSelf()
        }
        return START_NOT_STICKY
    }

    companion object {
        private const val TAG = "BandControl"
        private const val CHANNEL = "connection"
        private const val NOTIFICATION_ID = 1
        private const val ACTION_DISCONNECT = "dev.nikarh.bandcontrol.DISCONNECT"
        private const val EXTRA_TEXT = "text"

        /** Starts the service; only allowed while the app is visible. */
        fun start(
            context: Context,
            text: String,
        ) {
            val intent = Intent(context, ConnectionService::class.java).putExtra(EXTRA_TEXT, text)
            try {
                context.startForegroundService(intent)
            } catch (e: IllegalStateException) {
                Log.w(TAG, "can't start the connection service", e)
            }
        }

        /** Replaces the text of the running service's notification, also from the background. */
        fun update(
            context: Context,
            text: String,
        ) {
            val manager = context.getSystemService(NotificationManager::class.java) ?: return
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) {
                return
            }
            if (manager.activeNotifications.none { it.id == NOTIFICATION_ID }) return
            manager.notify(NOTIFICATION_ID, notification(context, text))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ConnectionService::class.java))
        }

        private fun notification(
            context: Context,
            text: String,
        ): Notification {
            val manager = context.getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(
                NotificationChannel(CHANNEL, "Connection to REAPER", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Shown while the app stays connected in the background"
                    setShowBadge(false)
                },
            )
            val open =
                PendingIntent.getActivity(
                    context,
                    0,
                    Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                    PendingIntent.FLAG_IMMUTABLE,
                )
            val disconnect =
                PendingIntent.getService(
                    context,
                    1,
                    Intent(context, ConnectionService::class.java).setAction(ACTION_DISCONNECT),
                    PendingIntent.FLAG_IMMUTABLE,
                )
            return NotificationCompat
                .Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(text)
                .setContentText("Stays connected for ${AppModel.BACKGROUND_MINUTES} minutes in the background")
                .setContentIntent(open)
                .addAction(0, "Disconnect", disconnect)
                .setOngoing(true)
                .setSilent(true)
                .setCategory(NotificationCompat.CATEGORY_SERVICE)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
                .build()
        }
    }
}

/** How the notification's Disconnect action reaches the [AppModel] of the running activity. */
internal object KeepAlive {
    @Volatile
    var disconnect: (() -> Unit)? = null
}
