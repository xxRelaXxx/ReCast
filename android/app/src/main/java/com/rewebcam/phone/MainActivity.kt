package com.rewebcam.phone

import android.Manifest
import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.view.Surface
import android.view.TextureView
import android.view.View
import android.view.WindowManager
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.SeekBar
import android.widget.Spinner
import android.widget.Switch
import android.widget.TextView
import android.widget.Toast
import kotlin.math.roundToInt

class MainActivity : Activity() {
    private var service: StreamService? = null
    private var bound = false
    private var pendingSurface: Surface? = null
    private var cameras: List<CameraSpec> = emptyList()
    private var suppressSpinner = false
    private var suppressControls = false
    private var zoomTracking = false
    private var autoConnectPending = false

    private lateinit var preview: TextureView
    private lateinit var statusText: TextView
    private lateinit var statsText: TextView
    private lateinit var hostInput: EditText
    private lateinit var portInput: EditText
    private lateinit var pinInput: EditText
    private lateinit var connectButton: Button
    private lateinit var cameraSpinner: Spinner
    private lateinit var resolutionSpinner: Spinner
    private lateinit var micSwitch: Switch
    private lateinit var torchSwitch: Switch
    private lateinit var zoomSeek: SeekBar

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            val svc = (binder as StreamService.LocalBinder).service()
            service = svc
            bound = true

            svc.statusListener = { text ->
                runOnUiThread {
                    statusText.text = text
                    updateStatusColor(text)
                }
            }
            svc.connectionListener = { connected ->
                runOnUiThread { updateConnectionUi(connected) }
            }
            svc.camerasListener = { list ->
                runOnUiThread { populateCameras(list) }
            }
            svc.statsListener = { text ->
                runOnUiThread {
                    statsText.text = text
                    syncPhoneState()
                }
            }
            svc.stateListener = {
                runOnUiThread { syncPhoneState() }
            }

            populateCameras(svc.cameraSpecs())
            preview.surfaceTexture?.let { texture ->
                val res = svc.selectedResolution()
                if (res != null) texture.setDefaultBufferSize(res.width, res.height)
            }
            pendingSurface?.let { svc.setPreviewSurface(it) }
            syncPhoneState()
            applyPreviewBufferSize()
            updatePreviewTransform()
            updateConnectionUi(svc.isConnected)

            if (autoConnectPending) {
                autoConnectPending = false
                if (!svc.isConnected) connect()
            }
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
            bound = false
            updateConnectionUi(false)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(R.layout.activity_main)
        bindViews()

        preview.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(
                surfaceTexture: SurfaceTexture,
                width: Int,
                height: Int
            ) {
                val res = service?.selectedResolution()
                surfaceTexture.setDefaultBufferSize(
                    res?.width ?: 1920,
                    res?.height ?: 1080
                )
                val surface = Surface(surfaceTexture)
                pendingSurface = surface
                service?.setPreviewSurface(surface)
                updatePreviewTransform()
            }

            override fun onSurfaceTextureSizeChanged(
                surfaceTexture: SurfaceTexture,
                width: Int,
                height: Int
            ) {
                // TextureView resets the default buffer size to the view size in
                // onSizeChanged(). Restore the camera resolution so the preview
                // stream keeps the same crop as the encoder stream.
                applyPreviewBufferSize()
                updatePreviewTransform()
            }

            override fun onSurfaceTextureDestroyed(surfaceTexture: SurfaceTexture): Boolean {
                service?.setPreviewSurface(null)
                pendingSurface?.release()
                pendingSurface = null
                return true
            }

            override fun onSurfaceTextureUpdated(surfaceTexture: SurfaceTexture) {
            }
        }

        connectButton.setOnClickListener { toggleConnection() }

        micSwitch.setOnCheckedChangeListener { _, checked ->
            if (suppressControls) return@setOnCheckedChangeListener
            service?.setMicEnabled(checked)
        }
        torchSwitch.setOnCheckedChangeListener { _, checked ->
            if (suppressControls) return@setOnCheckedChangeListener
            service?.setTorch(checked)
        }
        zoomSeek.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
                if (fromUser) service?.setZoom(1.0f + progress / 10.0f)
            }

            override fun onStartTrackingTouch(seekBar: SeekBar?) {
                zoomTracking = true
            }

            override fun onStopTrackingTouch(seekBar: SeekBar?) {
                zoomTracking = false
            }
        })

        val prefs = getSharedPreferences("rewebcam", Context.MODE_PRIVATE)
        hostInput.setText(prefs.getString("host", "") ?: "")
        pinInput.setText(prefs.getString("pin", "") ?: "")

        handleIntent(intent)
        requestRequiredPermissions()
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent != null) {
            handleIntent(intent)
            if (intent.getBooleanExtra("auto", false) && service != null) {
                connect()
            }
        }
    }

    private fun handleIntent(intent: Intent?) {
        intent ?: return
        val host = intent.getStringExtra("host")
        val pin = intent.getStringExtra("pin")
        if (!host.isNullOrEmpty()) hostInput.setText(host)
        if (!pin.isNullOrEmpty()) pinInput.setText(pin)
        if (intent.getBooleanExtra("auto", false)) autoConnectPending = true
    }

    private fun bindViews() {
        preview = findViewById(R.id.previewView)
        statusText = findViewById(R.id.statusText)
        statsText = findViewById(R.id.statsText)
        hostInput = findViewById(R.id.hostInput)
        portInput = findViewById(R.id.portInput)
        pinInput = findViewById(R.id.pinInput)
        connectButton = findViewById(R.id.connectButton)
        cameraSpinner = findViewById(R.id.cameraSpinner)
        resolutionSpinner = findViewById(R.id.resolutionSpinner)
        micSwitch = findViewById(R.id.micSwitch)
        torchSwitch = findViewById(R.id.torchSwitch)
        zoomSeek = findViewById(R.id.zoomSeek)
    }

    private fun applyPreviewBufferSize() {
        val res = service?.selectedResolution() ?: return
        preview.surfaceTexture?.setDefaultBufferSize(res.width, res.height)
    }

    private fun requestRequiredPermissions() {
        val needed = mutableListOf<String>()
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            needed.add(Manifest.permission.CAMERA)
        }
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            needed.add(Manifest.permission.RECORD_AUDIO)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            needed.add(Manifest.permission.POST_NOTIFICATIONS)
        }

        if (needed.isEmpty()) {
            startAndBind()
        } else {
            statusText.text = "Allow camera and microphone access to continue."
            requestPermissions(needed.toTypedArray(), REQUEST_PERMISSIONS)
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != REQUEST_PERMISSIONS) return

        val cameraGranted =
            checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        val micGranted =
            checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        if (cameraGranted && micGranted) {
            startAndBind()
        } else {
            statusText.text = "Camera and microphone permissions are required."
            statusText.setTextColor(0xFFFF6B6B.toInt())
            Toast.makeText(this, "Rewebcam needs camera and microphone access", Toast.LENGTH_LONG)
                .show()
        }
    }

    private fun startAndBind() {
        if (bound || service != null) return
        val intent = Intent(this, StreamService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
        bindService(intent, serviceConnection, Context.BIND_AUTO_CREATE)
        statusText.text = "Starting camera service…"
    }

    private fun toggleConnection() {
        val svc = service ?: return
        if (svc.isConnected) {
            svc.disconnect()
            statusText.text = "Disconnected"
            return
        }
        connect()
    }

    private fun connect() {
        val svc = service ?: run {
            autoConnectPending = true
            statusText.text = "Camera service is still starting…"
            return
        }
        val host = hostInput.text.toString().trim()
        val port = portInput.text.toString().toIntOrNull() ?: 47777
        val pin = pinInput.text.toString().trim()

        if (host.isEmpty()) {
            statusText.text = "Enter the PC address shown in Rewebcam."
            return
        }
        if (pin.length != 6) {
            statusText.text = "Enter the 6-digit PIN shown in Rewebcam."
            return
        }

        getSharedPreferences("rewebcam", Context.MODE_PRIVATE)
            .edit()
            .putString("host", host)
            .putString("pin", pin)
            .apply()

        svc.connect(host, port, pin)
    }

    private fun updateConnectionUi(connected: Boolean) {
        connectButton.text = if (connected) "Disconnect" else "Connect"
        hostInput.isEnabled = !connected
        portInput.isEnabled = !connected
        pinInput.isEnabled = !connected
        if (connected) {
            statusText.setTextColor(0xFF46D18C.toInt())
        } else {
            updateStatusColor(statusText.text.toString())
        }
    }

    private fun updateStatusColor(text: String) {
        val color = when {
            text.startsWith("Connected") -> 0xFF46D18C.toInt()
            text.startsWith("Error") || text.startsWith("Disconnected") -> 0xFFFF8A7A.toInt()
            text.startsWith("Connecting") || text.startsWith("Starting") -> 0xFFFFC857.toInt()
            else -> 0xFFAEB7C6.toInt()
        }
        statusText.setTextColor(color)
    }

    private fun populateCameras(list: List<CameraSpec>) {
        if (list.isEmpty()) {
            statusText.text = "No camera found on this phone."
            return
        }
        cameras = list
        suppressSpinner = true
        cameraSpinner.adapter = ArrayAdapter(
            this,
            android.R.layout.simple_spinner_dropdown_item,
            list.map { it.label }
        )
        val selectedId = service?.selectedCameraId() ?: ""
        val index = list.indexOfFirst { it.id == selectedId }.coerceAtLeast(0)
        cameraSpinner.setSelection(index)
        suppressSpinner = false

        populateResolutions(list[index])
        updatePreviewTransform()

        cameraSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(
                parent: AdapterView<*>?,
                view: View?,
                position: Int,
                id: Long
            ) {
                if (suppressSpinner) return
                val spec = cameras.getOrNull(position) ?: return
                service?.selectCamera(spec.id)
                populateResolutions(spec)
                updatePreviewTransform()
            }

            override fun onNothingSelected(parent: AdapterView<*>?) {
            }
        }
    }

    private fun populateResolutions(spec: CameraSpec) {
        suppressSpinner = true
        resolutionSpinner.adapter = ArrayAdapter(
            this,
            android.R.layout.simple_spinner_dropdown_item,
            spec.resolutions.map { "${it.width} × ${it.height}" }
        )
        val current = service?.selectedResolution()
        val initial = spec.resolutions.indexOfFirst {
            current != null && it.width == current.width && it.height == current.height
        }.coerceAtLeast(0)
        resolutionSpinner.setSelection(initial, false)
        suppressSpinner = false

        resolutionSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(
                parent: AdapterView<*>?,
                view: View?,
                position: Int,
                id: Long
            ) {
                if (suppressSpinner) return
                val resolution = spec.resolutions.getOrNull(position) ?: return
                val already = service?.selectedResolution()
                if (already != null &&
                    already.width == resolution.width && already.height == resolution.height
                ) {
                    return
                }
                preview.surfaceTexture?.setDefaultBufferSize(resolution.width, resolution.height)
                service?.selectResolution(resolution.width, resolution.height)
            }

            override fun onNothingSelected(parent: AdapterView<*>?) {
            }
        }
    }

    /**
     * Mirrors camera/microphone/torch/zoom state that may have been changed
     * from the desktop app or from the phone's own controls.
     */
    private fun syncPhoneState() {
        val svc = service ?: return
        val id = svc.selectedCameraId()
        val index = cameras.indexOfFirst { it.id == id }
        if (index < 0) return

        if (cameraSpinner.selectedItemPosition != index) {
            suppressSpinner = true
            cameraSpinner.setSelection(index, false)
            populateResolutions(cameras[index])
            suppressSpinner = false
        }

        val res = svc.selectedResolution()
        if (res != null) {
            val resIndex = cameras[index].resolutions.indexOfFirst {
                it.width == res.width && it.height == res.height
            }
            if (resIndex >= 0 && resolutionSpinner.selectedItemPosition != resIndex) {
                suppressSpinner = true
                resolutionSpinner.setSelection(resIndex, false)
                suppressSpinner = false
            }
        }

        suppressControls = true
        micSwitch.isChecked = svc.isMicOn()
        torchSwitch.isChecked = svc.isTorchOn()
        if (!zoomTracking) {
            val zoomProgress = ((svc.zoomRatio() - 1.0f) * 10.0f)
                .roundToInt()
                .coerceIn(0, zoomSeek.max)
            if (zoomSeek.progress != zoomProgress) {
                zoomSeek.progress = zoomProgress
            }
        }
        suppressControls = false
        applyPreviewBufferSize()
        updatePreviewTransform()
    }

    private fun updatePreviewTransform() {
        val viewWidth = preview.width.toFloat()
        val viewHeight = preview.height.toFloat()
        if (viewWidth <= 0f || viewHeight <= 0f) return

        val resolution = service?.selectedResolution()
        val bufferWidth = resolution?.width?.toFloat() ?: 1920f
        val bufferHeight = resolution?.height?.toFloat() ?: 1080f
        // The preview surface on this device applies an extra 90 degree
        // clockwise turn on its own, so compensate here. The reported rotation
        // still drives the desktop stream unchanged.
        val rotation = ((service?.previewRotation() ?: 0) + 270) % 360

        val rotatedWidth =
            if (rotation == 90 || rotation == 270) bufferHeight else bufferWidth
        val rotatedHeight =
            if (rotation == 90 || rotation == 270) bufferWidth else bufferHeight

        // TextureView initially stretches the camera buffer to the view. Undo
        // that stretch, rotate to the current display orientation, then scale
        // the upright image so it covers the view without letterboxing.
        val cover = maxOf(viewWidth / rotatedWidth, viewHeight / rotatedHeight)
        val cx = viewWidth / 2f
        val cy = viewHeight / 2f

        val matrix = Matrix()
        matrix.setScale(bufferWidth / viewWidth, bufferHeight / viewHeight, cx, cy)
        matrix.postRotate(rotation.toFloat(), cx, cy)
        matrix.postScale(cover, cover, cx, cy)

        val spec = service?.cameraSpecs()
            ?.firstOrNull { it.id == service?.selectedCameraId() }
        if (spec?.facing == "front") {
            matrix.postScale(-1f, 1f, cx, cy)
        }
        preview.setTransform(matrix)
    }

    override fun onDestroy() {
        if (bound) {
            try {
                unbindService(serviceConnection)
            } catch (_: Exception) {
            }
            bound = false
        }
        pendingSurface?.release()
        pendingSurface = null
        // The foreground service keeps streaming if the activity is swiped away.
        super.onDestroy()
    }

    companion object {
        private const val REQUEST_PERMISSIONS = 100
    }
}
