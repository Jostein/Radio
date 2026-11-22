package local.radio.android

import android.net.Uri
import java.util.UUID

data class Station(
    val id: String = UUID.randomUUID().toString(),
    val title: String,
    val url: String,
) {
    fun playableUri(): Uri? = playableUri(url)

    companion object {
        fun playableUri(raw: String): Uri? {
            val cleaned = raw.filterNot { it.isWhitespace() }
            if (cleaned.isEmpty()) return null
            val uri = runCatching { Uri.parse(cleaned) }.getOrNull() ?: return null
            val scheme = uri.scheme ?: return null
            if (!scheme.equals("http", ignoreCase = true) &&
                !scheme.equals("https", ignoreCase = true)
            ) {
                return null
            }
            return uri.takeIf { !it.host.isNullOrBlank() }
        }
    }
}
