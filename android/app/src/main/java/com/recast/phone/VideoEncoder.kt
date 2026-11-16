package com.recast.phone

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.os.Build
import android.os.Bundle
import android.view.Surface
import java.nio.ByteBuffer

class VideoEncoder(
    private val width: Int,
    private val height: Int,
    private val fps: Int,
    private val onFrame: (data: ByteArray, keyframe: Boolean, ptsUs: Long) -> Unit,
    private val onCsd: (data: ByteArray) -> Unit,
    private val onError: (message: String) -> Unit
) {
    companion object {
        private const val MIME = MediaFormat.MIMETYPE_VIDEO_AVC
    }

    private var codec: MediaCodec? = null

    var inputSurface: Surface? = null
        private set

    @Volatile
    private var running = false

    private var drainThread: Thread? = null

    @Volatile
    var framesSent = 0L
        private set

    @Volatile
    var bytesSent = 0L
        private set

    fun start() {
        val format = MediaFormat.createVideoFormat(MIME, width, height)
        format.setInteger(
            MediaFormat.KEY_COLOR_FORMAT,
            MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface
        )
        format.setInteger(MediaFormat.KEY_BIT_RATE, bitrateFor())
        format.setInteger(MediaFormat.KEY_FRAME_RATE, fps)
        format.setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
        format.setInteger(
            MediaFormat.KEY_BITRATE_MODE,
            MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_VBR
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            format.setInteger(MediaFormat.KEY_PREPEND_HEADER_TO_SYNC_FRAMES, 1)
        }

        val c = MediaCodec.createEncoderByType(MIME)
        c.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        inputSurface = c.createInputSurface()
        c.start()
        codec = c
        running = true
        drainThread = Thread({ drain(c) }, "recast-video").apply {
            isDaemon = true
            start()
        }
    }

    private fun drain(c: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        try {
            while (running) {
                val index = c.dequeueOutputBuffer(info, 10_000)
                when {
                    index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        val format = c.outputFormat
                        val csd0 = format.getByteBuffer("csd-0")?.let { copyBuffer(it) }
                        val csd1 = format.getByteBuffer("csd-1")?.let { copyBuffer(it) }
                        val combined = (csd0 ?: ByteArray(0)) + (csd1 ?: ByteArray(0))
                        if (combined.isNotEmpty()) onCsd(combined)
                    }

                    index >= 0 -> {
                        val buffer = c.getOutputBuffer(index)
                        if (buffer != null && info.size > 0) {
                            buffer.position(info.offset)
                            buffer.limit(info.offset + info.size)
                            val data = ByteArray(info.size)
                            buffer.get(data)
                            val isConfig =
                                (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0
                            val keyframe =
                                (info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0
                            if (isConfig) {
                                onCsd(data)
                            } else {
                                bytesSent += data.size
                                framesSent++
                                onFrame(data, keyframe, info.presentationTimeUs)
                            }
                        }
                        c.releaseOutputBuffer(index, false)
                    }
                }
            }
        } catch (e: Exception) {
            if (running) onError("Video encoder: ${e.message}")
        }
    }

    fun requestKeyframe() {
        try {
            val params = Bundle()
            params.putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0)
            codec?.setParameters(params)
        } catch (_: Exception) {
        }
    }

    fun stop() {
        running = false
        try {
            drainThread?.join(400)
        } catch (_: Exception) {
        }
        try {
            codec?.stop()
        } catch (_: Exception) {
        }
        try {
            codec?.release()
        } catch (_: Exception) {
        }
        codec = null
        try {
            inputSurface?.release()
        } catch (_: Exception) {
        }
        inputSurface = null
        drainThread = null
    }

    private fun bitrateFor(): Int = when {
        width * height >= 1920 * 1080 -> 6_000_000
        width * height >= 1280 * 720 -> 4_000_000
        else -> 2_500_000
    }

    private fun copyBuffer(buffer: ByteBuffer): ByteArray {
        buffer.rewind()
        val out = ByteArray(buffer.remaining())
        buffer.get(out)
        return out
    }
}
