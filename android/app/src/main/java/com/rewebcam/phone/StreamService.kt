package com.rewebcam.phone

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Binder
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.view.Surface

class StreamService : Service() {
    companion object {
        private const val CHANNEL_ID = "rewebcam_stream"
        private const val NOTIFICATION_ID = 47
    }

    inner class LocalBinder : Binder() {
        fun service(): StreamService = this@StreamService
    }

    private val binder = LocalBinder()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var controller: StreamController
    private var wakeLock: PowerManager.WakeLock? = null

    var statusListener: ((String) -> Unit)? = null
    var connectionListener: ((Boolean) -> Unit)? = null
    var camerasListener: ((List<CameraSpec>) -> Unit)? = null
    var statsListener: ((String) -> Unit)? = null
    var stateListener: (() -> Unit)? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        startForegroundCompat("Starting camera…")

        // Keep the CPU awake while the phone is used as a webcam: the screen
        // may be off during a long call, but the camera and network must keep
        // running.
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Rewebcam:stream")
        wakeLock?.setReferenceCounted(false)

        controller = StreamController(this, object : StreamController.Listener {
            override fun onStatus(text: String) {
                main.post {
                    statusListener?.invoke(text)
                    updateNotification(text)
                }
            }

            override fun onConnection(connected: Boolean) {
                main.post {
                    connectionListener?.invoke(connected)
                    updateNotification(if (connected) "Streaming to PC" else "Ready")
                }
            }

            override fun onCamerasChanged(cameras: List<CameraSpec>) {
                main.post { camerasListener?.invoke(cameras) }
            }

            override fun onStats(text: String) {
                main.post { statsListener?.invoke(text) }
            }

            override fun onStateChanged() {
                main.post { stateListener?.invoke() }
            }
        })
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

    override fun onDestroy() {
        try {
            controller.shutdown()
        } catch (_: Exception) {
        }
        releaseWakeLock()
        super.onDestroy()
    }

    fun setPreviewSurface(surface: Surface?) = controller.setPreviewSurface(surface)

    fun connect(host: String, port: Int, pin: String) {
        acquireWakeLock()
        controller.connect(host, port, pin)
    }

    fun disconnect() {
        controller.disconnect()
        releaseWakeLock()
    }

    fun selectCamera(id: String) = controller.selectCamera(id)

    fun selectResolution(width: Int, height: Int, fps: Int = 30) =
        controller.selectResolution(width, height, fps)

    fun requestKeyframe() = controller.requestKeyframe()

    fun previewRotation(): Int = controller.previewRotation()

    fun setMicEnabled(enabled: Boolean) = controller.setMicEnabled(enabled)

    fun setTorch(enabled: Boolean) = controller.setTorch(enabled)

    fun setZoom(value: Float) = controller.setZoom(value)

    fun cameraSpecs(): List<CameraSpec> = controller.cameras

    fun selectedCameraId(): String = controller.selectedCameraId

    fun selectedResolution(): CameraSpec.Resolution? = controller.selectedResolution

    fun isMicOn(): Boolean = controller.micOn

    fun isTorchOn(): Boolean = controller.torchOn

    fun zoomRatio(): Float = controller.zoomRatio

    val isConnected: Boolean
        get() = controller.isConnected

    private fun acquireWakeLock() {
        val lock = wakeLock ?: return
        if (!lock.isHeld) {
            lock.acquire()
        }
    }

    private fun releaseWakeLock() {
        val lock = wakeLock ?: return
        if (lock.isHeld) {
            lock.release()
        }
    }

    private fun createChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Phone camera stream",
            NotificationManager.IMPORTANCE_LOW
        )
        channel.description = "Shows when Rewebcam is using the camera and microphone"
        channel.setShowBadge(false)
        channel.lockscreenVisibility = Notification.VISIBILITY_PRIVATE
        manager.createNotificationChannel(channel)
    }

    private fun startForegroundCompat(text: String) {
        val notification = buildNotification(text)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val type = ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
            startForeground(NOTIFICATION_ID, notification, type)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun updateNotification(text: String) {
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildNotification(text))
    }

    private fun buildNotification(text: String): Notification {
        val intent = Intent(this, MainActivity::class.java)
        val pending = PendingIntent.getActivity(
            this,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("Rewebcam Phone")
            .setContentText(text)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(pending)
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_SERVICE)
            .build()
    }
}
