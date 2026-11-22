package local.radio.android

import android.app.Application
import android.content.ComponentName
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.AndroidViewModel
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture

class RadioViewModel(application: Application) : AndroidViewModel(application) {
    private val store = StationStore(application)

    var stations by mutableStateOf(store.load())
        private set

    var current by mutableStateOf<Station?>(null)
        private set

    var isPlaying by mutableStateOf(false)
        private set

    var status by mutableStateOf("Nothing playing")
        private set

    var track by mutableStateOf<String?>(null)
        private set

    private var controllerFuture: ListenableFuture<MediaController>? = null
    private var controller: MediaController? = null
    private var pendingStation: Station? = null

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(playing: Boolean) {
            isPlaying = playing
            updateStatus()
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            updateStatus()
        }

        override fun onPlayerError(error: PlaybackException) {
            status = error.localizedMessage ?: "Playback error"
            isPlaying = false
        }

        override fun onMediaMetadataChanged(mediaMetadata: MediaMetadata) {
            val title = mediaMetadata.title?.toString()?.trim().orEmpty()
            track = title.takeIf { it.isNotEmpty() && it != current?.title }
        }
    }

    init {
        connect()
    }

    private fun connect() {
        val token = SessionToken(
            getApplication(),
            ComponentName(getApplication(), PlaybackService::class.java),
        )
        val future = MediaController.Builder(getApplication(), token).buildAsync()
        controllerFuture = future
        future.addListener(
            {
                runCatching { future.get() }
                    .onSuccess { connected ->
                        controller = connected
                        connected.addListener(listener)
                        pendingStation?.let { station ->
                            pendingStation = null
                            play(station)
                        }
                        updateStatus()
                    }
                    .onFailure {
                        status = "Player unavailable: ${it.localizedMessage ?: "unknown error"}"
                    }
            },
            ContextCompat.getMainExecutor(getApplication()),
        )
    }

    fun play(station: Station) {
        current = station
        track = null
        val connected = controller
        if (connected == null) {
            pendingStation = station
            status = "Connecting player…"
            return
        }

        val uri = station.playableUri()
        if (uri == null) {
            status = "That stream URL isn’t valid."
            return
        }

        connected.setMediaItem(
            MediaItem.Builder()
                .setUri(uri)
                .setMediaId(station.id)
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle(station.title)
                        .setArtist("Radio")
                        .build()
                )
                .build()
        )
        connected.prepare()
        connected.play()
        updateStatus()
    }

    fun toggle() {
        val connected = controller ?: return
        if (connected.isPlaying) {
            connected.pause()
        } else {
            connected.play()
        }
        updateStatus()
    }

    fun add(station: Station) {
        stations = stations + station
        store.save(stations)
        play(station)
    }

    fun remove(station: Station) {
        if (current?.id == station.id) {
            controller?.stop()
            controller?.clearMediaItems()
            current = null
            track = null
        }
        stations = stations.filterNot { it.id == station.id }
        store.save(stations)
        updateStatus()
    }

    private fun updateStatus() {
        val connected = controller
        isPlaying = connected?.isPlaying == true
        status = when {
            connected == null -> "Connecting player…"
            connected.playerError != null -> connected.playerError?.localizedMessage ?: "Playback error"
            connected.playbackState == Player.STATE_BUFFERING -> "Connecting…"
            isPlaying -> "Live"
            current != null -> "Paused"
            else -> "Nothing playing"
        }
    }

    override fun onCleared() {
        controller?.removeListener(listener)
        controller?.release()
        controllerFuture?.let { MediaController.releaseFuture(it) }
        controller = null
        controllerFuture = null
        super.onCleared()
    }
}
