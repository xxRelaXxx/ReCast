package com.recast.phone

import android.Manifest
import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.content.res.ColorStateList
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
import android.widget.ImageView
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
    private lateinit var connectButton: View
    private lateinit var scanButton: View
    private lateinit var disconnectButton: Button
    private lateinit var connectionScreen: View
    private lateinit var controlScreen: View
    private lateinit var cameraSpinner: Spinner
    private lateinit var resolutionSpinner: Spinner
    private lateinit var cameraSwitch: Switch
    private lateinit var micSwitch: Switch
    private lateinit var torchSwitch: Switch
    private lateinit var zoomSeek: SeekBar
    private lateinit var cameraStateText: TextView
    private lateinit var micStateText: TextView
    private lateinit var cameraIcon: ImageView
    private lateinit var micIcon: ImageView
    private lateinit var cameraControls: View
    private lateinit var cameraOffOverlay: View

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            val svc = (binder as StreamService.LocalBinder).service()
            service = svc
            bound = true

            svc.statusListener = { text ->
                runOnUiThread {
                    statusText.text = text
                    updateStatusColor(text)
                    if (text.startsWith("Error") && controlScreen.visibility == View.VISIBLE) {
                        Toast.makeText(this@MainActivity, text, Toast.LENGTH_SHORT).show()
                    }
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
            if (!svc.isConnected && !autoConnectPending &&
                statusText.text.toString() == getString(R.string.status_starting)
            ) {
                statusText.text = getString(R.string.status_ready)
                statusText.setTextColor(getColor(R.color.app_text_muted))
            }
            maybeAutoConnect()
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
                val (bufferWidth, bufferHeight) = previewBufferSize()
                surfaceTexture.setDefaultBufferSize(bufferWidth, bufferHeight)
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

        connectButton.setOnClickListener { connect() }
        scanButton.setOnClickListener { openScanner() }
        disconnectButton.setOnClickListener {
            service?.disconnect()
            statusText.text = getString(R.string.status_ready)
            updateConnectionUi(false)
        }

        cameraSwitch.setOnCheckedChangeListener { _, checked ->
            if (suppressControls) return@setOnCheckedChangeListener
            service?.setVideoEnabled(checked)
        }
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

        val prefs = getSharedPreferences("recast", Context.MODE_PRIVATE)
        hostInput.setText(prefs.getString("host", "") ?: "")
        portInput.setText(prefs.getString("port", PairPayload.DEFAULT_PORT.toString()))
        pinInput.setText(prefs.getString("pin", "") ?: "")

        handleIntent(intent)
        requestRequiredPermissions()
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    /**
     * Accepts three ways of pairing: the text returned by the in-app scanner,
     * a `recast://pair` deep link, and the older extras used by early builds.
     */
    private fun handleIntent(intent: Intent?) {
        intent ?: return
        val scanned = intent.getStringExtra(QrScannerActivity.EXTRA_TEXT)
        val deepLink = if (intent.action == Intent.ACTION_VIEW) intent.dataString else null
        val payload = PairPayload.parse(scanned) ?: PairPayload.parse(deepLink)
        if (payload != null) {
            applyPairing(payload, autoConnect = true)
            return
        }

        val hosts = intent.getStringArrayListExtra("hosts")
        val pin = intent.getStringExtra("pin")
        if (!hosts.isNullOrEmpty() && !pin.isNullOrEmpty()) {
            applyPairing(
                PairPayload(
                    hosts,
                    intent.getIntExtra("port", PairPayload.DEFAULT_PORT),
                    pin
                ),
                autoConnect = true
            )
            return
        }

        val host = intent.getStringExtra("host")
        if (!host.isNullOrEmpty()) hostInput.setText(host)
        if (!pin.isNullOrEmpty()) pinInput.setText(pin)
        if (intent.getBooleanExtra("auto", false)) {
            autoConnectPending = true
            maybeAutoConnect()
        }
    }

    private fun openScanner() {
        startActivityForResult(
            Intent(this, QrScannerActivity::class.java),
            REQUEST_SCAN
        )
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQUEST_SCAN || resultCode != RESULT_OK) return
        val payload = PairPayload.parse(data?.getStringExtra(QrScannerActivity.EXTRA_TEXT))
            ?: return
        applyPairing(payload, autoConnect = true)
    }

    private fun applyPairing(payload: PairPayload, autoConnect: Boolean) {
        hostInput.setText(payload.hostList)
        portInput.setText(payload.port.toString())
        pinInput.setText(payload.pin)
        statusText.setTextColor(getColor(R.color.app_text_muted))
        statusText.text = "Pairing code received from ${payload.hosts.first()}."
        if (autoConnect) {
            autoConnectPending = true
            maybeAutoConnect()
        }
    }

    /** Connects as soon as the camera service is bound. */
    private fun maybeAutoConnect() {
        if (!autoConnectPending || !bound) return
        val svc = service ?: return
        autoConnectPending = false
        if (!svc.isConnected) connect()
    }

    private fun bindViews() {
        preview = findViewById(R.id.previewView)
        statusText = findViewById(R.id.statusText)
        statsText = findViewById(R.id.statsText)
        hostInput = findViewById(R.id.hostInput)
        portInput = findViewById(R.id.portInput)
        pinInput = findViewById(R.id.pinInput)
        connectButton = findViewById(R.id.connectButton)
        scanButton = findViewById(R.id.scanButton)
        disconnectButton = findViewById(R.id.disconnectButton)
        connectionScreen = findViewById(R.id.connectionScreen)
        controlScreen = findViewById(R.id.controlScreen)
        cameraSpinner = findViewById(R.id.cameraSpinner)
        resolutionSpinner = findViewById(R.id.resolutionSpinner)
        cameraSwitch = findViewById(R.id.cameraSwitch)
        micSwitch = findViewById(R.id.micSwitch)
        torchSwitch = findViewById(R.id.torchSwitch)
        zoomSeek = findViewById(R.id.zoomSeek)
        cameraStateText = findViewById(R.id.cameraStateText)
        micStateText = findViewById(R.id.micStateText)
        cameraIcon = findViewById(R.id.cameraIcon)
        micIcon = findViewById(R.id.micIcon)
        cameraControls = findViewById(R.id.cameraControls)
        cameraOffOverlay = findViewById(R.id.cameraOffOverlay)
    }

    private fun applyPreviewBufferSize() {
        val (width, height) = previewBufferSize()
        preview.surfaceTexture?.setDefaultBufferSize(width, height)
    }

    /**
     * The size of the buffer the camera fills.
     *
     * The camera hands the preview a frame that matches the orientation of
     * the screen, so in portrait the frame is turned and the buffer is
     * swapped. Requesting the stream size instead would squeeze the picture.
     */
    private fun previewBufferSize(): Pair<Int, Int> {
        val res = service?.selectedResolution()
        val width = res?.width ?: 1920
        val height = res?.height ?: 1080
        val rotation = displayRotationDegrees()
        return if (rotation == 90 || rotation == 270) {
            width to height
        } else {
            height to width
        }
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
            statusText.setTextColor(getColor(R.color.app_error))
            Toast.makeText(this, "ReCast needs camera and microphone access", Toast.LENGTH_LONG)
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
        statusText.text = getString(R.string.status_starting)
    }

    private fun connect() {
        val svc = service ?: run {
            autoConnectPending = true
            statusText.text = "Camera service is still starting…"
            return
        }
        val hosts = hostInput.text.toString()
            .split(',', ';', ' ', '\n')
            .map { it.trim().removePrefix("http://").removePrefix("https://") }
            .filter { it.isNotEmpty() }
        val port = portInput.text.toString().toIntOrNull() ?: PairPayload.DEFAULT_PORT
        val pin = pinInput.text.toString().trim()

        if (hosts.isEmpty()) {
            statusText.text = "Scan the QR code or enter the PC address."
            statusText.setTextColor(getColor(R.color.app_text_muted))
            return
        }
        if (port !in 1..65535) {
            statusText.text = "Enter a valid port, usually 47777."
            statusText.setTextColor(getColor(R.color.app_warning))
            return
        }
        if (pin.length != 6 || pin.any { !it.isDigit() }) {
            statusText.text = "Enter the 6-digit PIN shown in ReCast."
            statusText.setTextColor(getColor(R.color.app_warning))
            return
        }

        getSharedPreferences("recast", Context.MODE_PRIVATE)
            .edit()
            .putString("host", hosts.joinToString(","))
            .putString("port", port.toString())
            .putString("pin", pin)
            .apply()

        svc.connect(hosts, port, pin)
    }

    private fun updateConnectionUi(connected: Boolean) {
        connectButton.isEnabled = !connected
        scanButton.isEnabled = !connected
        hostInput.isEnabled = !connected
        portInput.isEnabled = !connected
        pinInput.isEnabled = !connected
        // The buttons are plain views, so the disabled look is set by hand.
        connectButton.alpha = if (connected) 0.4f else 1f
        scanButton.alpha = if (connected) 0.4f else 1f

        connectionScreen.visibility = if (connected) View.GONE else View.VISIBLE
        controlScreen.visibility = if (connected) View.VISIBLE else View.GONE

        if (connected) {
            statusText.setTextColor(getColor(R.color.app_live))
            preview.post { attachPreview() }
            // Show the current input modes as soon as the controls appear.
            syncPhoneState()
        } else {
            detachPreview()
            cameraOffOverlay.visibility = View.GONE
            updateStatusColor(statusText.text.toString())
        }
    }

    /**
     * The preview surface only exists while the controls are on screen, so the
     * camera stays closed on the pairing screen and reopens on connect.
     */
    private fun attachPreview() {
        val svc = service ?: return
        if (!preview.isAvailable) return
        if (pendingSurface == null) {
            val texture = preview.surfaceTexture ?: return
            val (width, height) = previewBufferSize()
            texture.setDefaultBufferSize(width, height)
            pendingSurface = Surface(texture)
        }
        svc.setPreviewSurface(pendingSurface)
        updatePreviewTransform()
    }

    private fun detachPreview() {
        service?.setPreviewSurface(null)
        pendingSurface?.release()
        pendingSurface = null
    }

    private fun updateStatusColor(text: String) {
        val color = when {
            text.startsWith("Connected") -> getColor(R.color.app_live)
            text.startsWith("Error") || text.startsWith("Disconnected") ->
                getColor(R.color.app_error)

            text.startsWith("Connecting") || text.startsWith("Starting") ->
                getColor(R.color.app_warning)

            else -> getColor(R.color.app_text_muted)
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
                preview.surfaceTexture?.let {
                    val rotation = displayRotationDegrees()
                    if (rotation == 90 || rotation == 270) {
                        it.setDefaultBufferSize(resolution.width, resolution.height)
                    } else {
                        it.setDefaultBufferSize(resolution.height, resolution.width)
                    }
                }
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

        if (index >= 0) {
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
        }

        val videoOn = svc.isVideoOn()
        val micOn = svc.isMicOn()

        suppressControls = true
        cameraSwitch.isChecked = videoOn
        micSwitch.isChecked = micOn
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

        // The input cards show the current mode of each device.
        cameraIcon.setImageResource(
            if (videoOn) R.drawable.ic_camera else R.drawable.ic_camera_off
        )
        cameraStateText.setText(
            if (videoOn) R.string.input_camera_on else R.string.input_camera_off
        )
        micIcon.setImageResource(if (micOn) R.drawable.ic_mic else R.drawable.ic_mic_off)
        micStateText.setText(if (micOn) R.string.input_mic_on else R.string.input_mic_off)

        // Without a camera there is nothing to configure.
        cameraControls.visibility = if (videoOn) View.VISIBLE else View.GONE
        cameraOffOverlay.visibility = if (videoOn) View.GONE else View.VISIBLE

        applyPreviewBufferSize()
        updatePreviewTransform()
    }

    /**
     * Fits the camera buffer into the preview without distortion.
     *
     * The camera hands the preview a frame that is already turned for the
     * screen, so the picture only needs the stretch of the TextureView
     * undone and a scale that fills the view instead of letterboxing it.
     */
    private fun updatePreviewTransform() {
        val viewWidth = preview.width.toFloat()
        val viewHeight = preview.height.toFloat()
        if (viewWidth <= 0f || viewHeight <= 0f) return

        val (bufferWidth, bufferHeight) = previewBufferSize()

        // How far the picture has to be turned for the screen.
        val turn = when (displayRotationDegrees()) {
            90 -> 270f
            180 -> 180f
            270 -> 90f
            else -> 0f
        }

        val uprightWidth = if (turn == 90f || turn == 270f) bufferHeight else bufferWidth
        val uprightHeight = if (turn == 90f || turn == 270f) bufferWidth else bufferHeight

        val centerX = viewWidth / 2f
        val centerY = viewHeight / 2f
        // Scale so the picture covers the view; the overflow is cropped.
        val cover = maxOf(viewWidth / uprightWidth, viewHeight / uprightHeight)

        val matrix = Matrix()
        matrix.setScale(bufferWidth / viewWidth, bufferHeight / viewHeight, centerX, centerY)
        matrix.postRotate(turn, centerX, centerY)
        matrix.postScale(cover, cover, centerX, centerY)
        if (isFrontCamera()) {
            matrix.postScale(-1f, 1f, centerX, centerY)
        }
        preview.setTransform(matrix)
    }

    /// The current rotation of the screen, in degrees.
    private fun displayRotationDegrees(): Int {
        val rotation = try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                display?.rotation ?: Surface.ROTATION_0
            } else {
                @Suppress("DEPRECATION")
                windowManager.defaultDisplay.rotation
            }
        } catch (_: Exception) {
            Surface.ROTATION_0
        }
        return when (rotation) {
            Surface.ROTATION_90 -> 90
            Surface.ROTATION_180 -> 180
            Surface.ROTATION_270 -> 270
            else -> 0
        }
    }

    private fun isFrontCamera(): Boolean {
        val svc = service ?: return false
        val id = svc.selectedCameraId()
        return svc.cameraSpecs().firstOrNull { it.id == id }?.facing == "front"
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
        private const val REQUEST_SCAN = 200
    }
}
