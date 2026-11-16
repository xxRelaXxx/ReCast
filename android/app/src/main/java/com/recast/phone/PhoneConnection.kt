package com.recast.phone

import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class PhoneConnection(
    private val hosts: List<String>,
    private val port: Int,
    private val pin: String,
    private val deviceName: String,
    private val listener: Listener
) {
    companion object {
        /** Each address gets its own short window before the next is tried. */
        private const val CONNECT_TIMEOUT_MS = 4000
    }

    interface Listener {
        fun onConnected()
        fun onDisconnected(reason: String)
        fun onControl(json: JSONObject)
        fun onStatus(message: String)
    }

    private val running = AtomicBoolean(false)
    private val connected = AtomicBoolean(false)
    private val queue = LinkedBlockingQueue<ByteArray>(512)

    @Volatile
    private var socket: Socket? = null

    @Volatile
    private var output: DataOutputStream? = null

    private var writer: Thread? = null
    private var reader: Thread? = null

    fun start() {
        if (!running.compareAndSet(false, true)) return
        reader = Thread({ readLoop() }, "recast-reader").apply {
            isDaemon = true
            start()
        }
    }

    private fun readLoop() {
        var reason = "Disconnected"
        try {
            val s = openSocket() ?: run {
                reason = "Could not reach the PC at port $port"
                return@readLoop
            }
            socket = s

            val out = DataOutputStream(BufferedOutputStream(s.getOutputStream()))
            output = out
            writer = Thread({ writeLoop() }, "recast-writer").apply {
                isDaemon = true
                start()
            }

            val hello = JSONObject()
                .put("pin", pin)
                .put("device", deviceName)
                .put("version", 1)
            out.write(Proto.jsonFrame(Proto.T_HELLO, 0, 0, hello))
            out.flush()

            val input = DataInputStream(BufferedInputStream(s.getInputStream()))
            while (running.get()) {
                val frame = Proto.readFrame(input) ?: break
                when (frame.type) {
                    Proto.T_HELLO_ACK -> {
                        val ack = Proto.json(frame.payload)
                        if (ack.optBoolean("ok")) {
                            connected.set(true)
                            listener.onConnected()
                        } else {
                            reason = ack.optString("message", "Pairing rejected")
                            break
                        }
                    }

                    Proto.T_CONTROL -> listener.onControl(Proto.json(frame.payload))
                    Proto.T_PING -> sendRaw(Proto.frame(Proto.T_PONG, 0, 0, frame.payload), true)
                    Proto.T_ERROR -> listener.onStatus(
                        Proto.json(frame.payload).optString("message", "Phone error")
                    )
                }
            }
        } catch (e: Exception) {
            reason = e.message ?: e.javaClass.simpleName
            if (running.get()) listener.onStatus("Connection error: $reason")
        } finally {
            shutdown(reason)
        }
    }

    /**
     * Opens the first reachable address. Public and private networks are both
     * in the QR payload, so a VPN or virtual adapter on the PC cannot stall
     * pairing behind a dead address.
     */
    private fun openSocket(): Socket? {
        if (hosts.isEmpty()) return null
        var lastError = ""
        for ((index, host) in hosts.withIndex()) {
            if (!running.get()) return null
            listener.onStatus(
                if (hosts.size == 1) {
                    "Connecting to $host:$port…"
                } else {
                    "Connecting to $host:$port (${index + 1}/${hosts.size})…"
                }
            )
            val socket = Socket()
            try {
                socket.tcpNoDelay = true
                socket.keepAlive = true
                socket.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS)
                return socket
            } catch (e: Exception) {
                lastError = e.message ?: e.javaClass.simpleName
                try {
                    socket.close()
                } catch (_: Exception) {
                }
            }
        }
        if (lastError.isNotEmpty()) {
            listener.onStatus("Network: $lastError")
        }
        return null
    }

    private fun writeLoop() {
        try {
            val out = output ?: return
            while (running.get()) {
                val bytes = queue.poll(200, TimeUnit.MILLISECONDS) ?: continue
                out.write(bytes)
                out.flush()
            }
        } catch (e: Exception) {
            if (running.get()) listener.onStatus("Send error: ${e.message}")
            shutdown("Connection lost")
        }
    }

    private fun sendRaw(bytes: ByteArray, reliable: Boolean) {
        if (!running.get()) return
        if (reliable) {
            if (!queue.offer(bytes, 300, TimeUnit.MILLISECONDS)) {
                queue.poll()
                queue.offer(bytes)
            }
        } else if (!queue.offer(bytes)) {
            // Network is behind: drop the oldest frame, never block the camera.
            queue.poll()
            queue.offer(bytes)
        }
    }

    fun sendVideo(data: ByteArray, flags: Int, ptsUs: Long) {
        sendRaw(Proto.frame(Proto.T_VIDEO, flags, ptsUs, data), false)
    }

    fun sendAudio(data: ByteArray, ptsUs: Long) {
        sendRaw(Proto.frame(Proto.T_AUDIO, 0, ptsUs, data), false)
    }

    fun sendCaps(json: JSONObject) {
        sendRaw(Proto.jsonFrame(Proto.T_CAPS, 0, 0, json), true)
    }

    fun sendVideoConfig(json: JSONObject) {
        sendRaw(Proto.jsonFrame(Proto.T_VIDEO_CONFIG, 0, 0, json), true)
    }

    fun sendStats(json: JSONObject) {
        sendRaw(Proto.jsonFrame(Proto.T_STATS, 0, 0, json), false)
    }

    fun isConnected(): Boolean = connected.get()

    fun stop() {
        shutdown("Disconnected by user")
    }

    @Synchronized
    private fun shutdown(reason: String) {
        if (!running.compareAndSet(true, false)) return
        val wasConnected = connected.getAndSet(false)
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        writer?.interrupt()
        socket = null
        output = null
        queue.clear()
        if (wasConnected || reason != "Disconnected by user") {
            listener.onDisconnected(reason)
        }
    }
}
