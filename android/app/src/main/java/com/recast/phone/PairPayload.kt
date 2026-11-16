package com.recast.phone

import java.net.URLDecoder

/**
 * Parsed `recast://pair` link from the QR code that the PC shows.
 *
 * The PC may list several addresses (Wi-Fi, Ethernet, VPN). The phone tries
 * every address in order, so a virtual adapter cannot block pairing.
 */
data class PairPayload(
    val hosts: List<String>,
    val port: Int,
    val pin: String
) {
    val hostList: String
        get() = hosts.joinToString(",")

    companion object {
        const val SCHEME = "recast://"
        const val DEFAULT_PORT = 47777

        /** Returns null when the text is not a valid ReCast pairing code. */
        fun parse(text: String?): PairPayload? {
            val raw = text?.trim().orEmpty()
            if (!raw.lowercase().startsWith(SCHEME)) return null
            val afterScheme = raw.substring(SCHEME.length)
            val head = afterScheme.substringBefore('?')
            if (!head.equals("pair", ignoreCase = true)) return null
            if (!afterScheme.contains('?')) return null

            val params = HashMap<String, String>()
            for (part in afterScheme.substringAfter('?').split('&')) {
                if (part.isEmpty()) continue
                val key = part.substringBefore('=').lowercase()
                if (key.isEmpty()) continue
                params[key] = decode(part.substringAfter('=', ""))
            }

            val hosts = params["hosts"].orEmpty()
                .split(',')
                .map { it.trim() }
                .filter { it.isNotEmpty() && it.length <= 255 }
            val pin = params["pin"].orEmpty().trim()
            if (hosts.isEmpty()) return null
            if (pin.length != 6 || pin.any { !it.isDigit() }) return null

            val port = params["port"]
                ?.toIntOrNull()
                ?.takeIf { it in 1..65535 }
                ?: DEFAULT_PORT

            return PairPayload(hosts, port, pin)
        }

        private fun decode(value: String): String = try {
            URLDecoder.decode(value, "UTF-8")
        } catch (_: Exception) {
            value
        }
    }
}
