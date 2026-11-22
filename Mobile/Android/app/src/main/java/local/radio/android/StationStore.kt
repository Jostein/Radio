package local.radio.android

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

class StationStore(context: Context) {
    private val preferences = context.getSharedPreferences("stations", Context.MODE_PRIVATE)

    private val starters = listOf(
        Station(
            title = "FIP Reggae",
            url = "https://stream.radiofrance.fr/fipreggae/fipreggae.m3u8",
        )
    )

    fun load(): List<Station> {
        val raw = preferences.getString(KEY, null) ?: return starters
        return runCatching {
            val array = JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val objectValue = array.optJSONObject(index) ?: continue
                    val title = objectValue.optString("title").trim()
                    val url = objectValue.optString("url").trim()
                    if (title.isEmpty() || url.isEmpty()) continue
                    add(
                        Station(
                            id = objectValue.optString("id").ifBlank { UUID.randomUUID().toString() },
                            title = title,
                            url = url,
                        )
                    )
                }
            }
        }.getOrElse { starters }
    }

    fun save(stations: List<Station>) {
        val array = JSONArray()
        for (station in stations) {
            array.put(
                JSONObject()
                    .put("id", station.id)
                    .put("title", station.title)
                    .put("url", station.url)
            )
        }
        preferences.edit().putString(KEY, array.toString()).apply()
    }

    private companion object {
        const val KEY = "stations"
    }
}
