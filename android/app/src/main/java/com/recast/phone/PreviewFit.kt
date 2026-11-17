package com.recast.phone

import kotlin.math.min

/**
 * How the connection preview is drawn.
 *
 * TextureView already turns the camera buffer upright for the phone's natural
 * pose, then stretches that picture across the whole view. The scale undoes
 * the stretch. The rotation only follows how the phone is held now, so the
 * picture stays upright. The frame itself stays tall on a vertical phone and
 * wide on a horizontal one. The whole frame stays visible: leftover space is
 * empty, never a crop and never a stretch.
 */
object PreviewFit {
    data class Draw(
        val scaleX: Float,
        val scaleY: Float,
        val rotationDegrees: Int
    )

    fun draw(
        viewWidth: Float,
        viewHeight: Float,
        bufferWidth: Float,
        bufferHeight: Float,
        frameRotationDegrees: Int,
        displayRotationDegrees: Int
    ): Draw? {
        if (viewWidth <= 0f || viewHeight <= 0f || bufferWidth <= 0f || bufferHeight <= 0f) {
            return null
        }
        val frame = Math.floorMod(frameRotationDegrees, 360)
        val swapped = frame == 90 || frame == 270
        val uprightWidth = if (swapped) bufferHeight else bufferWidth
        val uprightHeight = if (swapped) bufferWidth else bufferHeight
        val fit = min(viewWidth / uprightWidth, viewHeight / uprightHeight)
        // TextureView already applied the sensor turn. This undoes only the
        // turn of the phone, which is what was leaving the picture a quarter
        // turn clockwise.
        val turn = Math.floorMod(-displayRotationDegrees, 360)
        val turnSwaps = turn == 90 || turn == 270
        val preWidth = (if (turnSwaps) uprightHeight else uprightWidth) * fit
        val preHeight = (if (turnSwaps) uprightWidth else uprightHeight) * fit
        return Draw(
            scaleX = preWidth / viewWidth,
            scaleY = preHeight / viewHeight,
            rotationDegrees = turn
        )
    }
}
