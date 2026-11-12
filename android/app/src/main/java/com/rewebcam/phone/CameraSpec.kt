package com.rewebcam.phone

import org.json.JSONArray
import org.json.JSONObject

data class CameraSpec(
    val id: String,
    val label: String,
    val facing: String,
    val sensorOrientation: Int,
    val resolutions: List<Resolution>
) {
    data class Resolution(val width: Int, val height: Int)

    fun resolutionJson(): JSONArray {
        val arr = JSONArray()
        for (r in resolutions) {
            arr.put(
                JSONObject()
                    .put("width", r.width)
                    .put("height", r.height)
                    .put("fps", JSONArray().put(30))
            )
        }
        return arr
    }

    fun toJson(): JSONObject = JSONObject()
        .put("id", id)
        .put("label", label)
        .put("facing", facing)
        .put("resolutions", resolutionJson())
}
