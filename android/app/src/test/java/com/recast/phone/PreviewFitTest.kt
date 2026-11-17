package com.recast.phone

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs
import kotlin.math.min

class PreviewFitTest {
    @Test
    fun portraitFrameStaysTallAndThePictureIsNotTurnedFurther() {
        // A vertical phone. The stream rotation is 90°, which is the tall
        // frame. TextureView already turned the sensor, so the picture
        // itself is not turned again.
        val shown = shown(1080f, 1600f, 1920f, 1080f, frameRotation = 90, displayRotation = 0)
        assertAspect(1080f, 1920f, shown.width, shown.height)
        assertFits(1080f, 1600f, shown.width, shown.height)
        assertEquals(0, shown.rotation)
    }

    @Test
    fun landscapeFrameStaysWideAndThePictureFollowsThePhone() {
        val shown = shown(1800f, 900f, 1920f, 1080f, frameRotation = 0, displayRotation = 90)
        assertAspect(1920f, 1080f, shown.width, shown.height)
        assertFits(1800f, 900f, shown.width, shown.height)
        assertEquals(270, shown.rotation)
    }

    @Test
    fun upsideDownLandscapeKeepsTheWideFrame() {
        val shown = shown(1800f, 900f, 1920f, 1080f, frameRotation = 180, displayRotation = 270)
        assertAspect(1920f, 1080f, shown.width, shown.height)
        assertFits(1800f, 900f, shown.width, shown.height)
        assertEquals(90, shown.rotation)
    }

    @Test
    fun fourByThreeKeepsItsShapeWhenThePhoneIsVertical() {
        val shown = shown(1080f, 2000f, 640f, 480f, frameRotation = 270, displayRotation = 0)
        assertAspect(480f, 640f, shown.width, shown.height)
        assertFits(1080f, 2000f, shown.width, shown.height)
        assertEquals(0, shown.rotation)
    }

    @Test
    fun portraitAndLandscapeShowTheSamePicture() {
        val portrait = shown(1080f, 1800f, 1280f, 720f, frameRotation = 90, displayRotation = 0)
        val landscape = shown(1800f, 1080f, 1280f, 720f, frameRotation = 0, displayRotation = 90)
        val portraitAspect = portrait.width / portrait.height
        val landscapeAspect = landscape.width / landscape.height
        // One is the other turned on its side, so the ratios are reciprocals.
        assertEquals(portraitAspect, 1f / landscapeAspect, 0.001f)
    }

    @Test
    fun rejectsAnEmptyView() {
        assertNull(PreviewFit.draw(0f, 100f, 1920f, 1080f, 90, 0))
    }

    private data class Shown(val width: Float, val height: Float, val rotation: Int)

    private fun shown(
        viewWidth: Float,
        viewHeight: Float,
        bufferWidth: Float,
        bufferHeight: Float,
        frameRotation: Int,
        displayRotation: Int
    ): Shown {
        val draw = PreviewFit.draw(
            viewWidth,
            viewHeight,
            bufferWidth,
            bufferHeight,
            frameRotation,
            displayRotation
        )!!
        val preWidth = viewWidth * draw.scaleX
        val preHeight = viewHeight * draw.scaleY
        val quarter = Math.floorMod(draw.rotationDegrees, 360)
        val (width, height) = if (quarter == 90 || quarter == 270) {
            preHeight to preWidth
        } else {
            preWidth to preHeight
        }
        return Shown(width, height, draw.rotationDegrees)
    }

    private fun assertAspect(expectedWidth: Float, expectedHeight: Float, width: Float, height: Float) {
        val expected = expectedWidth / expectedHeight
        val actual = width / height
        assertEquals(expected, actual, 0.001f)
    }

    private fun assertFits(viewWidth: Float, viewHeight: Float, width: Float, height: Float) {
        assertTrue(width <= viewWidth + 0.5f)
        assertTrue(height <= viewHeight + 0.5f)
        val limit = min(viewWidth, viewHeight)
        val longest = maxOf(width, height)
        // The frame touches the tighter side, so it is fitted rather than shrunk away.
        assertTrue(
            abs(longest - maxOf(viewWidth, viewHeight)) < 0.5f ||
                abs(minOf(width, height) - limit) < 0.5f ||
                abs(width - viewWidth) < 0.5f ||
                abs(height - viewHeight) < 0.5f
        )
    }
}
