package com.recast.phone

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.ImageReader
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.util.Size
import android.view.Surface
import android.view.TextureView
import android.view.View
import android.view.WindowManager
import android.widget.TextView
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * In-app QR scanner. It uses Camera2 and the offline ZXing core, so pairing
 * never depends on Google Play services or an internet connection.
 */
class QrScannerActivity : Activity() {
    companion object {
        const val EXTRA_TEXT = "scan_text"
        private const val REQUEST_CAMERA = 301
        private const val TAG = "ReCastScanner"
    }

    private lateinit var preview: TextureView
    private lateinit var hint: TextView
    private lateinit var manager: CameraManager

    private var camera: CameraDevice? = null
    private var session: CameraCaptureSession? = null
    private var reader: ImageReader? = null
    private var previewSurface: Surface? = null

    private var thread: HandlerThread? = null
    private var handler: Handler? = null
    private val decoder = Executors.newSingleThreadExecutor()
    private val decoding = AtomicBoolean(false)
    private val delivered = AtomicBoolean(false)

    private var active = false
    private var frontCamera = false
    private var sensorOrientation = 90
    private var previewSize = Size(1280, 720)
    private var captureSize = Size(1280, 720)

    private val zxing = MultiFormatReader().apply {
        // Only these two hints work on this picture, the pure barcode mode
        // misses the code completely.
        setHints(
            mapOf(
                DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
                DecodeHintType.TRY_HARDER to true
            )
        )
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(R.layout.activity_qr_scanner)

        preview = findViewById(R.id.scanPreview)
        hint = findViewById(R.id.scanHint)
        findViewById<View>(R.id.scanClose).setOnClickListener { finish() }

        manager = getSystemService(CAMERA_SERVICE) as CameraManager
        thread = HandlerThread("recast-qr").apply { start() }
        handler = Handler(thread!!.looper)

        preview.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(
                surfaceTexture: SurfaceTexture,
                width: Int,
                height: Int
            ) {
                val (bufferWidth, bufferHeight) = previewBufferSize()
                surfaceTexture.setDefaultBufferSize(bufferWidth, bufferHeight)
                previewSurface?.release()
                previewSurface = Surface(surfaceTexture)
                openCamera()
            }

            override fun onSurfaceTextureSizeChanged(
                surfaceTexture: SurfaceTexture,
                width: Int,
                height: Int
            ) {
                applyPreviewTransform()
            }

            override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
                closeCamera()
                previewSurface?.release()
                previewSurface = null
                return true
            }

            override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) {
            }
        }

        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.CAMERA), REQUEST_CAMERA)
        }
    }

    override fun onResume() {
        super.onResume()
        active = true
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            openCamera()
        }
    }

    override fun onPause() {
        active = false
        closeCamera()
        super.onPause()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != REQUEST_CAMERA) return
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            openCamera()
        } else {
            hint.text = getString(R.string.scanner_permission)
        }
    }

    private fun openCamera() {
        if (!active || camera != null || previewSurface == null || !preview.isAvailable) return
        val id = pickCamera() ?: run {
            hint.text = getString(R.string.scanner_no_camera)
            return
        }
        try {
            val characteristics = manager.getCameraCharacteristics(id)
            frontCamera = characteristics.get(CameraCharacteristics.LENS_FACING) ==
                CameraCharacteristics.LENS_FACING_FRONT
            sensorOrientation =
                characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 90
            chooseSizes(characteristics)
            val (bufferWidth, bufferHeight) = previewBufferSize()
            preview.surfaceTexture?.setDefaultBufferSize(bufferWidth, bufferHeight)
            applyPreviewTransform()
            createReader()
            manager.openCamera(id, object : CameraDevice.StateCallback() {
                override fun onOpened(device: CameraDevice) {
                    if (!active) {
                        device.close()
                        return
                    }
                    camera = device
                    createSession(device)
                }

                override fun onDisconnected(device: CameraDevice) {
                    device.close()
                    if (camera === device) camera = null
                }

                override fun onError(device: CameraDevice, error: Int) {
                    device.close()
                    if (camera === device) camera = null
                    hint.text = getString(R.string.scanner_camera_error)
                }
            }, handler)
        } catch (e: SecurityException) {
            hint.text = getString(R.string.scanner_permission)
        } catch (e: Exception) {
            hint.text = getString(R.string.scanner_camera_error)
        }
    }

    private fun pickCamera(): String? {
        val ids = manager.cameraIdList
        return ids.firstOrNull {
            manager.getCameraCharacteristics(it)
                .get(CameraCharacteristics.LENS_FACING) == CameraCharacteristics.LENS_FACING_BACK
        } ?: ids.firstOrNull()
    }

    private fun chooseSizes(characteristics: CameraCharacteristics) {
        val map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP) ?: return
        val previewSizes = map.getOutputSizes(SurfaceTexture::class.java) ?: return
        previewSize = bestSize(previewSizes, 1280, 720)
        val captureSizes = map.getOutputSizes(android.graphics.ImageFormat.JPEG) ?: return
        // A larger picture makes the code easier to read.
        captureSize = bestSize(captureSizes, 1920, 1080)
    }

    /** Largest size up to the preferred one, or the closest smaller size. */
    private fun bestSize(sizes: Array<Size>, maxWidth: Int, maxHeight: Int): Size {
        val fitting = sizes.filter { it.width <= maxWidth && it.height <= maxHeight }
        return fitting.maxByOrNull { it.width.toLong() * it.height }
            ?: sizes.minByOrNull { it.width.toLong() * it.height }
            ?: Size(maxWidth, maxHeight)
    }

    private fun createReader() {
        reader?.close()
        val created = ImageReader.newInstance(
            captureSize.width,
            captureSize.height,
            android.graphics.ImageFormat.JPEG,
            2
        )
        created.setOnImageAvailableListener({ source ->
            if (delivered.get() || !decoding.compareAndSet(false, true)) {
                source.acquireLatestImage()?.close()
                return@setOnImageAvailableListener
            }
            val image = try {
                source.acquireLatestImage()
            } catch (_: Exception) {
                null
            }
            if (image == null) {
                decoding.set(false)
                return@setOnImageAvailableListener
            }
            try {
                val buffer = image.planes[0].buffer
                buffer.rewind()
                val bytes = ByteArray(buffer.remaining())
                buffer.get(bytes)
                Log.d(TAG, "scanner picture ${bytes.size} bytes")
                decoder.execute { decode(bytes) }
            } catch (_: Exception) {
                decoding.set(false)
            } finally {
                image.close()
            }
        }, handler)
        reader = created
    }

    private fun createSession(device: CameraDevice) {
        val previewTarget = previewSurface ?: return
        val captureTarget = reader?.surface ?: return
        try {
            device.createCaptureSession(
                listOf(previewTarget, captureTarget),
                object : CameraCaptureSession.StateCallback() {
                    override fun onConfigured(configured: CameraCaptureSession) {
                        if (camera == null) {
                            configured.close()
                            return
                        }
                        session = configured
                        try {
                            // The code is read from a still picture, so the
                            // session needs the template that produces them.
                            val request = device.createCaptureRequest(CameraDevice.TEMPLATE_STILL_CAPTURE)
                            request.addTarget(previewTarget)
                            request.addTarget(captureTarget)
                            request.set(
                                CaptureRequest.CONTROL_AF_MODE,
                                CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE
                            )
                            configured.setRepeatingRequest(request.build(), null, handler)
                        } catch (e: Exception) {
                            hint.text = getString(R.string.scanner_camera_error)
                        }
                    }

                    override fun onConfigureFailed(configured: CameraCaptureSession) {
                        hint.text = getString(R.string.scanner_camera_error)
                    }
                },
                handler
            )
        } catch (e: Exception) {
            hint.text = getString(R.string.scanner_camera_error)
        }
    }

    private fun decode(bytes: ByteArray) {
        try {
            val options = BitmapFactory.Options().apply { inSampleSize = 1 }
            val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: return
            val rotation = displayRotation()
            val upright = if (rotation == 0) {
                bitmap
            } else {
                Bitmap.createBitmap(
                    bitmap,
                    0,
                    0,
                    bitmap.width,
                    bitmap.height,
                    Matrix().apply { postRotate(rotation.toFloat()) },
                    true
                )
            }

            // The turned picture is read first, the original is the fallback
            // for cameras that deliver the code the other way round.
            var found = readCode(upright)
            if (found == null && upright !== bitmap) {
                found = readCode(bitmap)
            }
            val payload = found?.let { PairPayload.parse(it) }
            if (payload != null) {
                Log.d(TAG, "pairing code found")
                runOnUiThread { deliver(payload) }
            } else if (found != null) {
                runOnUiThread { hint.text = getString(R.string.scanner_not_recast) }
            }
            if (upright !== bitmap) upright.recycle()
            bitmap.recycle()
        } catch (_: Exception) {
            // Most frames simply do not contain a code yet.
        } finally {
            zxing.reset()
            decoding.set(false)
        }
    }

    /// Looks for a code in the picture and returns its text.
    private fun readCode(bitmap: Bitmap): String? {
        val width = bitmap.width
        val height = bitmap.height
        if (width <= 0 || height <= 0) return null
        val pixels = IntArray(width * height)
        bitmap.getPixels(pixels, 0, width, 0, 0, width, height)
        val source = RGBLuminanceSource(width, height, pixels)
        return try {
            zxing.decodeWithState(BinaryBitmap(HybridBinarizer(source))).text
        } catch (_: Exception) {
            null
        }
    }

    private fun deliver(payload: PairPayload) {
        if (isFinishing || isDestroyed) return
        if (!delivered.compareAndSet(false, true)) return
        hint.text = getString(R.string.scanner_success, payload.hosts.first())
        preview.postDelayed({
            val data = Intent().apply {
                putStringArrayListExtra("hosts", ArrayList(payload.hosts))
                putExtra("port", payload.port)
                putExtra("pin", payload.pin)
                putExtra(
                    EXTRA_TEXT,
                    "recast://pair?hosts=${payload.hostList}&port=${payload.port}&pin=${payload.pin}"
                )
            }
            setResult(RESULT_OK, data)
            finish()
        }, 350)
    }

    private fun displayRotation(): Int {
        val rotation = try {
            when (windowManager.defaultDisplay.rotation) {
                Surface.ROTATION_90 -> 90
                Surface.ROTATION_180 -> 180
                Surface.ROTATION_270 -> 270
                else -> 0
            }
        } catch (_: Exception) {
            0
        }
        return if (frontCamera) {
            (sensorOrientation + rotation) % 360
        } else {
            (sensorOrientation - rotation + 360) % 360
        }
    }

    private fun applyPreviewTransform() {
        val viewWidth = preview.width.toFloat()
        val viewHeight = preview.height.toFloat()
        if (viewWidth <= 0f || viewHeight <= 0f) return
        // The camera hands the preview a frame that is turned for the screen,
        // so the buffer is the size of the screen and the picture is only
        // scaled to fill the view.
        val (bufferWidth, bufferHeight) = previewBufferSize()
        val turn = when (displayRotationDegrees()) {
            90 -> 270f
            180 -> 180f
            270 -> 90f
            else -> 0f
        }
        val uprightWidth = if (turn == 90f || turn == 270f) bufferHeight else bufferWidth
        val uprightHeight = if (turn == 90f || turn == 270f) bufferWidth else bufferHeight

        val cover = maxOf(viewWidth / uprightWidth, viewHeight / uprightHeight)
        val cx = viewWidth / 2f
        val cy = viewHeight / 2f
        val matrix = Matrix()
        matrix.setScale(bufferWidth / viewWidth, bufferHeight / viewHeight, cx, cy)
        matrix.postRotate(turn, cx, cy)
        matrix.postScale(cover, cover, cx, cy)
        if (frontCamera) matrix.postScale(-1f, 1f, cx, cy)
        preview.setTransform(matrix)
    }

    /// The size of the preview buffer, turned like the screen.
    private fun previewBufferSize(): Pair<Int, Int> {
        val rotation = displayRotationDegrees()
        return if (rotation == 90 || rotation == 270) {
            previewSize.width to previewSize.height
        } else {
            previewSize.height to previewSize.width
        }
    }

    /// The rotation of the screen in degrees.
    private fun displayRotationDegrees(): Int {
        val rotation = try {
            when (windowManager.defaultDisplay.rotation) {
                Surface.ROTATION_90 -> 90
                Surface.ROTATION_180 -> 180
                Surface.ROTATION_270 -> 270
                else -> 0
            }
        } catch (_: Exception) {
            0
        }
        return rotation
    }

    private fun closeCamera() {
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
            camera?.close()
        } catch (_: Exception) {
        }
        camera = null
        reader?.close()
        reader = null
    }

    override fun onDestroy() {
        active = false
        decoder.shutdownNow()
        closeCamera()
        thread?.quitSafely()
        thread = null
        handler = null
        super.onDestroy()
    }
}
