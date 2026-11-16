package com.recast.phone

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairPayloadTest {
    @Test
    fun parsesTheDesktopPayload() {
        val payload = PairPayload.parse(
            "recast://pair?hosts=192.168.1.82,10.0.0.5&port=47777&pin=123456"
        )
        assertEquals(listOf("192.168.1.82", "10.0.0.5"), payload?.hosts)
        assertEquals(47777, payload?.port)
        assertEquals("123456", payload?.pin)
    }

    @Test
    fun keepsEveryAddressInOrder() {
        val payload = PairPayload.parse(
            "recast://pair?hosts=10.0.0.5,192.168.1.82,192.168.1.82&pin=654321"
        )
        assertEquals(listOf("10.0.0.5", "192.168.1.82", "192.168.1.82"), payload?.hosts)
        assertEquals(PairPayload.DEFAULT_PORT, payload?.port)
    }

    @Test
    fun rejectsOtherSchemesAndCodes() {
        assertNull(PairPayload.parse("https://example.com/pair?hosts=1.2.3.4&pin=123456"))
        assertNull(PairPayload.parse("recast://other?hosts=1.2.3.4&pin=123456"))
        assertNull(PairPayload.parse("just some text"))
    }

    @Test
    fun rejectsIncompletePairingData() {
        assertNull(PairPayload.parse("recast://pair?hosts=1.2.3.4&pin=12345"))
        assertNull(PairPayload.parse("recast://pair?hosts=1.2.3.4&pin=abcdef"))
        assertNull(PairPayload.parse("recast://pair?port=47777&pin=123456"))
    }

    @Test
    fun decodesEscapedValues() {
        val payload = PairPayload.parse(
            "recast://pair?hosts=192.168.1.82%2C10.0.0.5&port=50000&pin=000000"
        )
        assertEquals(listOf("192.168.1.82", "10.0.0.5"), payload?.hosts)
        assertEquals(50000, payload?.port)
    }
}
