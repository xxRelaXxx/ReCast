package com.recast.phone

import android.content.Context
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

class StreamController(
    private val context: Context,
    private val listener: Listener
) {
    companion object {
        private const val TAG = "ReCastStream"
    }

    interface Listener {
        fun onStatus(text: String)
        fun onConnection(connected: Boolean)
        fun onCamerasChanged(cameras: List<CameraSpec>)
        fun onStats(text: String)
        fun onStateChanged()
    }

    private val camera = CameraController(context, object : CameraController.Events {
        override fun status(message: String) {
            this@StreamController.status(message)
        }

        override fun error(message: String) {
            this@StreamController.status("Error: $message")
        }

        override fun videoConfig(
            width: Int,
            height: Int,
            fps: Int,
            rotation: Int,
            mirror: Boolean
        ) {
            if (!videoEnabled) return
            val conn = connection ?: return
            val json = JSONObject()
                .put("codec", "h264")
                .put("width", width)
                .put("height", height)
                .put("fps", fps)
                .put("rotation", rotation)
                .put("mirror", mirror)
            configSent = true
            conn.sendVideoConfig(json)
            pendingCsd?.let {
                conn.sendVideo(data = it, flags = Proto.VF_CONFIG, ptsUs = 0)
                pendingCsd = null
            }
        }

        override fun encodedFrame(data: ByteArray, keyframe: Boolean, ptsUs: Long) {
            if (!configSent || !videoEnabled) return
            val conn = connection ?: return
            encodedBytes += data.size
            val flags = if (keyframe) Proto.VF_KEYFRAME else 0
            conn.sendVideo(data, flags, ptsUs)
        }

        override fun codecConfig(data: ByteArray) {
            if (!videoEnabled) return
            if (!configSent) {
                pendingCsd = data
                return
            }
            val conn = connection ?: return
            conn.sendVideo(data, Proto.VF_CONFIG, 0)
        }

        override fun stateChanged() {
            listener.onStateChanged()
        }
    })

    private val audio = AudioCapture(
        onAudio = { data, ptsUs ->
            connection?.sendAudio(data, ptsUs)
            audioBytes += data.size
        },
        onError = { message -> status("Error: $message") }
    )

    private var connection: PhoneConnection? = null
    private var micEnabled = true
    private var videoEnabled = true
    private var started = false
    private var configSent = false
    private var pendingCsd: ByteArray? = null

    @Volatile
    private var running = true

    @Volatile
    private var encodedBytes = 0L

    @Volatile
    private var audioBytes = 0L

    private var statsThread: Thread? = null

    val cameras: List<CameraSpec>
        get() = camera.cameras

    val selectedCameraId: String
        get() = camera.selected?.id ?: ""

    val isConnected: Boolean
        get() = connection?.isConnected() == true

    val selectedResolution: CameraSpec.Resolution?
        get() = camera.selectedResolution

    val micOn: Boolean
        get() = micEnabled

    val videoOn: Boolean
        get() = videoEnabled

    val torchOn: Boolean
        get() = camera.torchEnabled

    val zoomRatio: Float
        get() = camera.zoomRatio

    init {
        listener.onCamerasChanged(camera.cameras)
        statsThread = Thread({ statsLoop() }, "recast-stats").apply {
            isDaemon = true
            start()
        }
    }

    fun setPreviewSurface(surface: android.view.Surface?) {
        camera.setPreviewSurface(surface)
    }

    fun connect(hosts: List<String>, port: Int, pin: String) {
        disconnect()

        var connRef: PhoneConnection? = null
        val conn = PhoneConnection(
            hosts = hosts,
            port = port,
            pin = pin,
            deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".trim(),
            listener = object : PhoneConnection.Listener {
                override fun onConnected() {
                    Log.d(TAG, "connected")
                    status("Connected to PC")
                    listener.onConnection(true)
                    connRef?.sendCaps(capsJson())
                    // Inputs start when the desktop sends Start, after it has
                    // sent the selected camera and microphone modes. A
                    // microphone-only session therefore never opens the camera.
                }

                override fun onDisconnected(reason: String) {
                    Log.d(TAG, "disconnected: $reason")
                    if (reason != "Disconnected by user") {
                        status("Disconnected: $reason")
                    }
                    listener.onConnection(false)
                    started = false
                    camera.setStreaming(false)
                    audio.stop()
                    configSent = false
                    if (connection === connRef) connection = null
                }

                override fun onControl(json: JSONObject) {
                    handleControl(json)
                }

                override fun onStatus(message: String) {
                    status(message)
                }
            }
        )
        connRef = conn

        connection = conn
        configSent = false
        status(
            if (hosts.size == 1) "Connecting to ${hosts.first()}:$port…"
            else "Connecting to ${hosts.first()}:$port and ${hosts.size - 1} more…"
        )
        conn.start()
    }

    fun disconnect() {
        val conn = connection
        connection = null
        conn?.stop()
        configSent = false
        started = false
        camera.setStreaming(false)
        audio.stop()
        listener.onConnection(false)
    }

    fun selectCamera(id: String) {
        camera.selectCamera(id)
    }

    fun selectResolution(width: Int, height: Int, fps: Int = 30) {
        camera.selectResolution(width, height, fps)
    }

    fun requestKeyframe() = camera.requestKeyframe()

    fun previewRotation(): Int = camera.currentRotation()

    fun setMicEnabled(enabled: Boolean) {
        Log.d(TAG, "setMicEnabled $enabled")
        if (micEnabled == enabled) return
        micEnabled = enabled
        audio.enabled = enabled
        if (enabled && started && isConnected) {
            audio.start()
        } else if (!enabled) {
            audio.stop()
        }
        listener.onStateChanged()
        sendInputState()
    }

    /**
     * Turns the camera and video stream on or off. When off, the camera device
     * is closed, so a microphone-only session does not power the camera.
     */
    fun setVideoEnabled(enabled: Boolean) {
        Log.d(TAG, "setVideoEnabled $enabled")
        if (videoEnabled == enabled) return
        videoEnabled = enabled
        if (!enabled) {
            configSent = false
            pendingCsd = null
        }
        camera.setVideoEnabled(enabled)
        listener.onStateChanged()
        sendInputState()
    }

    fun setTorch(enabled: Boolean) {
        camera.setTorch(enabled)
    }

    fun setZoom(value: Float) {
        camera.setZoom(value)
    }

    fun shutdown() {
        running = false
        disconnect()
        camera.shutdown()
        audio.stop()
        try {
            statsThread?.join(300)
        } catch (_: Exception) {
        }
        statsThread = null
    }

    private fun handleControl(json: JSONObject) {
        Log.d(TAG, "control $json")
        when (json.optString("cmd")) {
            "set_camera" -> camera.selectCamera(json.optString("id"))
            "set_resolution" -> camera.selectResolution(
                json.optInt("width", 1280),
                json.optInt("height", 720),
                json.optInt("fps", 30)
            )

            "set_video" -> setVideoEnabled(json.optBoolean("enabled", true))
            "set_mic" -> setMicEnabled(json.optBoolean("enabled", true))
            "set_torch" -> camera.setTorch(json.optBoolean("enabled", false))
            "set_zoom" -> camera.setZoom(json.optDouble("value", 1.0).toFloat())
            "request_keyframe" -> camera.requestKeyframe()
            "start" -> {
                started = true
                camera.setStreaming(true)
                if (micEnabled) audio.start()
            }

            "stop" -> {
                started = false
                camera.setStreaming(false)
                audio.stop()
            }
        }
    }

    private fun capsJson(): JSONObject {
        val camerasJson = JSONArray()
        for (spec in camera.cameras) {
            camerasJson.put(spec.toJson())
        }
        return JSONObject()
            .put("cameras", camerasJson)
            .put("microphone", true)
            .put("device", "${Build.MANUFACTURER} ${Build.MODEL}".trim())
            .put("camera_id", camera.selected?.id ?: "")
            .put("width", camera.selectedResolution?.width ?: 0)
            .put("height", camera.selectedResolution?.height ?: 0)
            .put("fps", camera.streamFps)
            .put("mic", micEnabled)
            .put("video", videoEnabled)
    }

    /// Sends the current input modes immediately instead of waiting for the
    /// next two-second stats tick, so the desktop tiles react at once.
    private fun sendInputState() {
        val conn = connection ?: return
        if (!conn.isConnected()) return
        val stats = JSONObject()
            .put("fps", camera.streamFps.toFloat())
            .put("camera_id", camera.selected?.id ?: "")
            .put("width", camera.selectedResolution?.width ?: 0)
            .put("height", camera.selectedResolution?.height ?: 0)
            .put("mic", micEnabled)
            .put("video", videoEnabled)
            .put("torch", camera.torchEnabled)
            .put("zoom", camera.zoomRatio.toDouble())
        conn.sendStats(stats)
    }

    private fun statsLoop() {
        var lastBytes = 0L
        var lastTime = System.currentTimeMillis()
        while (running) {
            try {
                Thread.sleep(2000)
            } catch (_: InterruptedException) {
                break
            }
            if (!running) break
            val conn = connection ?: continue
            if (!conn.isConnected()) continue

            val now = System.currentTimeMillis()
            val elapsed = ((now - lastTime).coerceAtLeast(1)).toDouble() / 1000.0
            val bytes = encodedBytes + audioBytes
            val kbps = (((bytes - lastBytes) * 8.0 / 1000.0) / elapsed).toInt()
            lastBytes = bytes
            lastTime = now

            val battery = batteryLevel()
            val thermal = thermalStatus()
            val stats = JSONObject()
                .put("battery", if (battery >= 0) battery else JSONObject.NULL)
                .put("thermal", thermal)
                .put("fps", camera.streamFps.toFloat())
                .put("bitrate_kbps", kbps)
                .put("camera_id", camera.selected?.id ?: "")
                .put("width", camera.selectedResolution?.width ?: 0)
                .put("height", camera.selectedResolution?.height ?: 0)
                .put("mic", micEnabled)
                .put("video", videoEnabled)
                .put("torch", camera.torchEnabled)
                .put("zoom", camera.zoomRatio.toDouble())
                .put("dropped", 0)
            conn.sendStats(stats)

            val text = buildString {
                append("Connected")
                if (videoEnabled) {
                    val res = camera.selectedResolution
                    if (res != null) append(" · ${res.width}×${res.height}")
                    append(" · ${camera.streamFps} fps")
                    append(" · $kbps kbps")
                } else {
                    append(" · camera off")
                }
                if (battery >= 0) append(" · battery $battery%")
                append(if (micEnabled) " · mic on" else " · mic off")
            }
            listener.onStats(text)
        }
    }

    private fun batteryLevel(): Int {
        return try {
            val bm = context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager
            bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
        } catch (_: Exception) {
            -1
        }
    }

    private fun thermalStatus(): Int {
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
                pm.currentThermalStatus
            } else {
                0
            }
        } catch (_: Exception) {
            0
        }
    }

    private fun status(message: String) {
        listener.onStatus(message)
    }
}
