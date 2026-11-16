package com.recast.phone

import org.json.JSONObject
import java.io.DataInputStream
import java.io.EOFException
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * ReCast wire protocol, shared with the Rust desktop app.
 * Header: magic "RWBC" (4) | type (1) | flags (1) | reserved (2) | length (4) | pts_us (8), little endian.
 */
object Proto {
    const val MAGIC = "RWBC"
    const val HEADER_LEN = 20
    const val MAX_PAYLOAD = 32 * 1024 * 1024

    const val T_HELLO = 1
    const val T_HELLO_ACK = 2
    const val T_CAPS = 3
    const val T_VIDEO_CONFIG = 4
    const val T_VIDEO = 5
    const val T_AUDIO = 6
    const val T_CONTROL = 7
    const val T_STATS = 8
    const val T_PING = 9
    const val T_PONG = 10
    const val T_ERROR = 11

    const val VF_CONFIG = 0x01
    const val VF_KEYFRAME = 0x02

    data class Frame(val type: Int, val flags: Int, val ptsUs: Long, val payload: ByteArray)

    fun frame(type: Int, flags: Int, ptsUs: Long, payload: ByteArray): ByteArray {
        require(payload.size <= MAX_PAYLOAD) { "payload too large" }
        val out = ByteBuffer.allocate(HEADER_LEN + payload.size).order(ByteOrder.LITTLE_ENDIAN)
        out.put(MAGIC.toByteArray(Charsets.US_ASCII))
        out.put(type.toByte())
        out.put(flags.toByte())
        out.putShort(0)
        out.putInt(payload.size)
        out.putLong(ptsUs)
        out.put(payload)
        return out.array()
    }

    fun jsonFrame(type: Int, flags: Int, ptsUs: Long, json: JSONObject): ByteArray =
        frame(type, flags, ptsUs, json.toString().toByteArray(Charsets.UTF_8))

    @Throws(IOException::class)
    fun readFrame(input: DataInputStream): Frame? {
        val header = ByteArray(HEADER_LEN)
        try {
            input.readFully(header)
        } catch (_: EOFException) {
            return null
        }
        val buf = ByteBuffer.wrap(header).order(ByteOrder.LITTLE_ENDIAN)
        val magic = ByteArray(4)
        buf.get(magic)
        if (String(magic, Charsets.US_ASCII) != MAGIC) throw IOException("bad magic")
        val type = buf.get().toInt() and 0xff
        val flags = buf.get().toInt() and 0xff
        buf.short // reserved
        val length = buf.int
        if (length < 0 || length > MAX_PAYLOAD) throw IOException("bad payload length")
        val pts = buf.long
        val payload = ByteArray(length)
        input.readFully(payload)
        return Frame(type, flags, pts, payload)
    }

    fun json(payload: ByteArray): JSONObject = JSONObject(String(payload, Charsets.UTF_8))
}
