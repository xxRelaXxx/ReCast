package com.recast.phone

import android.content.Context
import android.graphics.Rect
import android.graphics.SurfaceTexture
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CameraMetadata
import android.hardware.camera2.CaptureRequest
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.util.Range
import android.view.Surface
import android.view.WindowManager

class CameraController(
    private val context: Context,
    private val events: Events
) {
    companion object {
        private const val TAG = "ReCastCamera"
    }

    interface Events {
        fun status(message: String)
        fun error(message: String)
        fun videoConfig(width: Int, height: Int, fps: Int, rotation: Int, mirror: Boolean)
        fun encodedFrame(data: ByteArray, keyframe: Boolean, ptsUs: Long)
        fun codecConfig(data: ByteArray)
        fun stateChanged()
    }

    private val manager = context.getSystemService(Context.CAMERA_SERVICE) as CameraManager
    private val thread = HandlerThread("recast-camera").apply { start() }
    private val handler = Handler(thread.looper)

    var cameras: List<CameraSpec> = emptyList()
        private set

    @Volatile
    var selected: CameraSpec? = null
        private set

    @Volatile
    var selectedResolution: CameraSpec.Resolution? = null
        private set

    @Volatile
    var streamFps = 30
        private set

    @Volatile
    var isStreaming = false
        private set

    @Volatile
    var videoEnabled = true
        private set

    private var previewSurface: Surface? = null
    private var device: CameraDevice? = null
    private var session: CameraCaptureSession? = null
    private var encoder: VideoEncoder? = null
    @Volatile
    private var torchOn = false
    @Volatile
    private var zoom = 1.0f
    private var closed = false
    private var generation = 0
    private val reopenRunnable = Runnable { reopenNow() }

    init {
        enumerate()
    }

    private fun enumerate() {
        val list = mutableListOf<CameraSpec>()
        try {
            for (id in manager.cameraIdList) {
                val characteristics = manager.getCameraCharacteristics(id)
                val facing = when (characteristics.get(CameraCharacteristics.LENS_FACING)) {
                    CameraCharacteristics.LENS_FACING_FRONT -> "front"
                    CameraCharacteristics.LENS_FACING_BACK -> "back"
                    else -> "external"
                }
                val orientation = characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 0
                val map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                val sizes = map?.getOutputSizes(SurfaceTexture::class.java)?.toList() ?: emptyList()

                val standard = listOf(
                    1920 to 1080,
                    1280 to 720,
                    854 to 480,
                    640 to 480,
                    640 to 360,
                    320 to 240
                )
                var resolutions = standard
                    .filter { (w, h) -> sizes.any { it.width == w && it.height == h } }
                    .map { CameraSpec.Resolution(it.first, it.second) }

                if (resolutions.isEmpty()) {
                    resolutions = sizes
                        .filter {
                            it.width % 2 == 0 && it.height % 2 == 0 &&
                                it.width <= 1920 && it.height <= 1080
                        }
                        .sortedByDescending { it.width.toLong() * it.height }
                        .take(5)
                        .map { CameraSpec.Resolution(it.width, it.height) }
                }

                if (resolutions.isNotEmpty()) {
                    list.add(
                        CameraSpec(
                            id = id,
                            label = when (facing) {
                                "front" -> "Front camera"
                                "back" -> "Back camera"
                                else -> "Camera $id"
                            },
                            facing = facing,
                            sensorOrientation = orientation,
                            resolutions = resolutions
                        )
                    )
                }
            }
        } catch (e: Exception) {
            events.error("Camera list: ${e.message}")
        }

        cameras = list
        if (selected == null) {
            selected = list.firstOrNull { it.facing == "back" } ?: list.firstOrNull()
            selectedResolution = selected?.resolutions?.firstOrNull()
        }
    }

    fun setPreviewSurface(surface: Surface?) {
        handler.post {
            previewSurface = surface
            scheduleReopen()
        }
    }

    fun selectCamera(id: String) {
        handler.post {
            val spec = cameras.firstOrNull { it.id == id } ?: return@post
            if (spec.id == selected?.id) return@post
            Log.d(TAG, "selectCamera $id (${spec.label}) was ${selected?.id}")
            if (torchOn && !hasFlash(spec.id)) torchOn = false
            val previous = selectedResolution
            selected = spec
            selectedResolution = previous
                ?.takeIf { old -> spec.resolutions.any { it.width == old.width && it.height == old.height } }
                ?: spec.resolutions.firstOrNull()
            events.stateChanged()
            scheduleReopen()
        }
    }

    fun selectResolution(width: Int, height: Int, fps: Int) {
        handler.post {
            val spec = selected ?: return@post
            val resolution = spec.resolutions.firstOrNull {
                it.width == width && it.height == height
            } ?: return@post
            if (resolution == selectedResolution && fps == streamFps) return@post
            Log.d(TAG, "selectResolution ${width}x$height @$fps (was ${selectedResolution?.width}x${selectedResolution?.height} @$streamFps)")
            selectedResolution = resolution
            streamFps = fps.coerceIn(15, 60)
            events.stateChanged()
            scheduleReopen()
        }
    }

    fun setStreaming(streaming: Boolean) {
        handler.post {
            if (isStreaming == streaming) return@post
            Log.d(TAG, "setStreaming $streaming")
            isStreaming = streaming
            scheduleReopen()
        }
    }

    /**
     * Opens or closes the camera for the selected video mode. When disabled,
     * the camera device is closed completely, so a microphone-only session
     * leaves the camera unpowered.
     */
    fun setVideoEnabled(enabled: Boolean) {
        handler.post {
            if (videoEnabled == enabled) return@post
            Log.d(TAG, "setVideoEnabled $enabled")
            videoEnabled = enabled
            if (!enabled) {
                torchOn = false
                selected?.let { spec ->
                    try {
                        manager.setTorchMode(spec.id, false)
                    } catch (_: Exception) {
                    }
                }
            }
            scheduleReopen()
        }
    }

    /**
     * Camera and resolution controls usually arrive together. Wait a moment so
     * they are applied in one camera open instead of reopening twice.
     */
    private fun scheduleReopen() {
        handler.removeCallbacks(reopenRunnable)
        handler.postDelayed(reopenRunnable, 120)
    }

    val torchEnabled: Boolean
        get() = torchOn

    val zoomRatio: Float
        get() = zoom

    fun setTorch(enabled: Boolean) {
        handler.post {
            if (!videoEnabled) {
                events.error("Turn the camera on first")
                return@post
            }
            val id = selected?.id ?: return@post
            if (enabled && !hasFlash(id)) {
                events.error("Torch is not available on this camera")
                return@post
            }
            torchOn = enabled
            if (session != null) {
                // The camera is open by this app, so setTorchMode() is refused
                // with CAMERA_IN_USE. While streaming the flash is driven by
                // the capture request instead.
                applyRepeatingRequest()
            } else {
                try {
                    manager.setTorchMode(id, enabled)
                } catch (e: Exception) {
                    events.error("Torch: ${e.message}")
                    torchOn = false
                }
            }
            events.stateChanged()
        }
    }

    private fun hasFlash(id: String): Boolean = try {
        manager.getCameraCharacteristics(id)
            .get(CameraCharacteristics.FLASH_INFO_AVAILABLE) == true
    } catch (_: Exception) {
        false
    }

    fun setZoom(value: Float) {
        handler.post {
            zoom = value.coerceIn(1.0f, 8.0f)
            if (session != null) applyRepeatingRequest()
            events.stateChanged()
        }
    }

    /// Rotation needed to display the camera buffer upright for the current
    /// display orientation. Mirrors the value reported to the desktop.
    fun currentRotation(): Int {
        val spec = selected ?: return 0
        return computeRotation(spec)
    }

    fun requestKeyframe() {
        handler.post { encoder?.requestKeyframe() }
    }

    fun shutdown() {
        handler.post {
            closed = true
            generation++
            handler.removeCallbacks(reopenRunnable)
            closeSession()
            thread.quitSafely()
        }
    }

    private fun reopenNow() {
        generation++
        val gen = generation
        closeSession()
        if (closed) return
        if (!videoEnabled) return

        val spec = selected ?: return
        val resolution = selectedResolution ?: spec.resolutions.firstOrNull() ?: return
        if (previewSurface == null && !isStreaming) return

        if (isStreaming) {
            val enc = VideoEncoder(
                resolution.width,
                resolution.height,
                streamFps,
                onFrame = { data, keyframe, pts ->
                    events.encodedFrame(data, keyframe, pts)
                },
                onCsd = { data -> events.codecConfig(data) },
                onError = { message -> events.error(message) }
            )
            try {
                enc.start()
            } catch (e: Exception) {
                events.error("Video encoder: ${e.message}")
                return
            }
            encoder = enc
            val rotation = computeRotation(spec)
            Log.d(
                TAG,
                "stream ${spec.label} id=${spec.id} facing=${spec.facing} " +
                    "${resolution.width}x${resolution.height}@$streamFps rotation=$rotation " +
                    "mirror=${spec.facing == "front"}"
            )
            events.videoConfig(
                resolution.width,
                resolution.height,
                streamFps,
                rotation,
                spec.facing == "front"
            )
        }

        // CameraDevice.close() is asynchronous on Android; give the sensor a
        // moment to be released before opening the next configuration.
        handler.postDelayed({
            if (gen == generation && !closed) {
                openCamera(spec, gen)
            }
        }, 180)
    }

    private fun openCamera(spec: CameraSpec, gen: Int) {
        try {
            manager.openCamera(spec.id, object : CameraDevice.StateCallback() {
                override fun onOpened(camera: CameraDevice) {
                    if (closed || gen != generation) {
                        camera.close()
                        return
                    }
                    device = camera
                    createSession(camera, gen)
                    if (torchOn) {
                        try {
                            manager.setTorchMode(spec.id, true)
                        } catch (_: Exception) {
                        }
                    }
                }

                override fun onDisconnected(camera: CameraDevice) {
                    camera.close()
                    if (device === camera) device = null
                    events.status("Camera disconnected")
                    if (gen == generation && !closed) scheduleReopen()
                }

                override fun onError(camera: CameraDevice, error: Int) {
                    camera.close()
                    if (device === camera) device = null
                    events.error("Camera error $error")
                    if (gen == generation && !closed) scheduleReopen()
                }
            }, handler)
        } catch (e: SecurityException) {
            events.error("Camera permission missing")
        } catch (e: Exception) {
            events.error("Open camera: ${e.message}")
        }
    }

    @Suppress("DEPRECATION")
    private fun createSession(camera: CameraDevice, gen: Int) {
        val surfaces = mutableListOf<Surface>()
        previewSurface?.let { if (it.isValid) surfaces.add(it) }
        encoder?.inputSurface?.let { if (it.isValid) surfaces.add(it) }
        if (surfaces.isEmpty()) {
            events.status("Camera ready")
            return
        }

        try {
            camera.createCaptureSession(
                surfaces,
                object : CameraCaptureSession.StateCallback() {
                    override fun onConfigured(captureSession: CameraCaptureSession) {
                        if (device == null || closed || gen != generation) {
                            captureSession.close()
                            return
                        }
                        session = captureSession
                        applyRepeatingRequest()
                        val res = selectedResolution
                        if (res != null) {
                            events.status("Camera ready: ${res.width}x${res.height}")
                        }
                    }

                    override fun onConfigureFailed(captureSession: CameraCaptureSession) {
                        if (gen != generation) return
                        Log.e(TAG, "capture session configure failed at ${selectedResolution?.width}x${selectedResolution?.height}")
                        events.error("Camera session configuration failed")
                        val res = selectedResolution ?: return
                        val spec = selected ?: return
                        if (res.width > 1280 || res.height > 720) {
                            val fallback = spec.resolutions.firstOrNull {
                                it.width == 1280 && it.height == 720
                            }
                            if (fallback != null) {
                                Log.d(TAG, "falling back to 1280x720")
                                selectedResolution = fallback
                                scheduleReopen()
                            }
                        }
                    }
                },
                handler
            )
        } catch (e: Exception) {
            events.error("Capture session: ${e.message}")
        }
    }

    private fun applyRepeatingRequest() {
        val camera = device ?: return
        val captureSession = session ?: return
        try {
            val builder = camera.createCaptureRequest(CameraDevice.TEMPLATE_RECORD)
            previewSurface?.let { if (it.isValid) builder.addTarget(it) }
            encoder?.inputSurface?.let { if (it.isValid) builder.addTarget(it) }
            builder.set(CaptureRequest.CONTROL_MODE, CameraMetadata.CONTROL_MODE_AUTO)
            if (selected != null && hasFlash(selected!!.id)) {
                builder.set(
                    CaptureRequest.FLASH_MODE,
                    if (torchOn) CameraMetadata.FLASH_MODE_TORCH else CameraMetadata.FLASH_MODE_OFF
                )
            }
            try {
                builder.set(
                    CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE,
                    Range(streamFps, streamFps)
                )
            } catch (_: Exception) {
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                builder.set(CaptureRequest.CONTROL_ZOOM_RATIO, zoom)
            } else if (zoom > 1.0f && selected != null) {
                val characteristics =
                    manager.getCameraCharacteristics(selected!!.id)
                val active = characteristics.get(
                    CameraCharacteristics.SENSOR_INFO_ACTIVE_ARRAY_SIZE
                )
                if (active != null) {
                    val cropWidth = (active.width() / zoom).toInt()
                    val cropHeight = (active.height() / zoom).toInt()
                    val cx = active.centerX()
                    val cy = active.centerY()
                    builder.set(
                        CaptureRequest.SCALER_CROP_REGION,
                        Rect(
                            cx - cropWidth / 2,
                            cy - cropHeight / 2,
                            cx + cropWidth / 2,
                            cy + cropHeight / 2
                        )
                    )
                }
            }

            captureSession.setRepeatingRequest(builder.build(), null, handler)
        } catch (e: Exception) {
            events.error("Preview: ${e.message}")
        }
    }

    private fun closeSession() {
        try {
            session?.stopRepeating()
        } catch (_: Exception) {
        }
        try {
            session?.close()
        } catch (_: Exception) {
        }
        session = null
        try {
            device?.close()
        } catch (_: Exception) {
        }
        device = null
        encoder?.stop()
        encoder = null
    }

    private fun computeRotation(spec: CameraSpec): Int {
        val displayRotation = try {
            val wm = context.getSystemService(Context.WINDOW_SERVICE) as WindowManager
            when (wm.defaultDisplay.rotation) {
                Surface.ROTATION_90 -> 90
                Surface.ROTATION_180 -> 180
                Surface.ROTATION_270 -> 270
                else -> 0
            }
        } catch (_: Exception) {
            0
        }
        val sensor = spec.sensorOrientation
        val rotation = if (spec.facing == "front") {
            (sensor + displayRotation) % 360
        } else {
            (sensor - displayRotation + 360) % 360
        }
        Log.d(
            TAG,
            "rotation sensor=$sensor display=$displayRotation facing=${spec.facing} => $rotation"
        )
        return ((rotation / 90) * 90) % 360
    }
}
