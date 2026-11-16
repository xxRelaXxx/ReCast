package com.recast.phone

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.nio.ByteBuffer
import java.nio.ByteOrder

class AudioCapture(
    private val onAudio: (data: ByteArray, ptsUs: Long) -> Unit,
    private val onError: (message: String) -> Unit
) {
    companion object {
        const val SAMPLE_RATE = 48_000
        const val FRAME_SAMPLES = 960 // 20 ms mono
    }

    @Volatile
    var enabled = true

    @Volatile
    var bytesSent = 0L
        private set

    @Volatile
    private var running = false

    private var thread: Thread? = null
    private var record: AudioRecord? = null

    fun start() {
        if (running) return
        val minBuffer = AudioRecord.getMinBufferSize(
            SAMPLE_RATE,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT
        )
        if (minBuffer <= 0) {
            onError("AudioRecord is not available on this device")
            return
        }

        val rec = try {
            AudioRecord(
                MediaRecorder.AudioSource.MIC,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                minBuffer * 2
            )
        } catch (e: Exception) {
            onError("Audio init: ${e.message}")
            return
        }

        if (rec.state != AudioRecord.STATE_INITIALIZED) {
            rec.release()
            onError("AudioRecord failed to initialize")
            return
        }

        record = rec
        running = true
        try {
            rec.startRecording()
        } catch (e: Exception) {
            running = false
            rec.release()
            record = null
            onError("Audio start: ${e.message}")
            return
        }

        thread = Thread({ loop(rec) }, "recast-audio").apply {
            isDaemon = true
            start()
        }
    }

    private fun loop(rec: AudioRecord) {
        val samples = ShortArray(FRAME_SAMPLES)
        val bytes = ByteArray(FRAME_SAMPLES * 2)
        var ptsUs = 0L
        try {
            while (running) {
                val count = rec.read(samples, 0, samples.size)
                if (count > 0) {
                    if (enabled) {
                        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
                        buffer.asShortBuffer().put(samples, 0, count)
                        onAudio(bytes.copyOf(count * 2), ptsUs)
                        bytesSent += count * 2
                    }
                    ptsUs += (count * 1_000_000L) / SAMPLE_RATE
                }
            }
        } catch (e: Exception) {
            if (running) onError("Audio capture: ${e.message}")
        }
    }

    fun stop() {
        running = false
        try {
            thread?.join(500)
        } catch (_: Exception) {
        }
        try {
            record?.stop()
        } catch (_: Exception) {
        }
        try {
            record?.release()
        } catch (_: Exception) {
        }
        record = null
        thread = null
    }
}
