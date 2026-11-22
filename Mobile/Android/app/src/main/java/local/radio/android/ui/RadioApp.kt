package local.radio.android.ui

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import local.radio.android.RadioViewModel
import local.radio.android.Station

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RadioApp(viewModel: RadioViewModel = viewModel()) {
    var showAdd by rememberSaveable { mutableStateOf(false) }
    var addFocused by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Radio") },
                actions = {
                    IconButton(
                        onClick = { showAdd = true },
                        modifier = Modifier
                            .onFocusChanged { addFocused = it.isFocused }
                            .then(
                                if (addFocused) {
                                    Modifier.border(
                                        2.dp,
                                        MaterialTheme.colorScheme.primary,
                                        CircleShape,
                                    )
                                } else {
                                    Modifier
                                }
                            ),
                    ) {
                        Icon(Icons.Default.Add, contentDescription = "Add station")
                    }
                },
            )
        },
    ) { innerPadding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                NowPlayingCard(
                    title = viewModel.current?.title,
                    track = viewModel.track,
                    status = viewModel.status,
                    isPlaying = viewModel.isPlaying,
                    canPlay = viewModel.current != null,
                    onToggle = viewModel::toggle,
                )
            }

            if (viewModel.stations.isEmpty()) {
                item {
                    Text(
                        text = "No stations yet. Add a stream URL to start listening.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            items(viewModel.stations, key = { it.id }) { station ->
                StationRow(
                    station = station,
                    isCurrent = viewModel.current?.id == station.id,
                    onPlay = { viewModel.play(station) },
                    onDelete = { viewModel.remove(station) },
                )
            }

            item {
                var addButtonFocused by remember { mutableStateOf(false) }
                OutlinedButton(
                    onClick = { showAdd = true },
                    modifier = Modifier
                        .fillMaxWidth()
                        .onFocusChanged { addButtonFocused = it.isFocused }
                        .then(
                            if (addButtonFocused) {
                                Modifier.border(
                                    2.dp,
                                    MaterialTheme.colorScheme.primary,
                                    RoundedCornerShape(20.dp),
                                )
                            } else {
                                Modifier
                            }
                        ),
                ) {
                    Icon(Icons.Default.Add, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text("Add station")
                }
            }
        }
    }

    if (showAdd) {
        AddStationDialog(
            onDismiss = { showAdd = false },
            onAdd = { station ->
                viewModel.add(station)
                showAdd = false
            },
        )
    }
}

@Composable
private fun NowPlayingCard(
    title: String?,
    track: String?,
    status: String,
    isPlaying: Boolean,
    canPlay: Boolean,
    onToggle: () -> Unit,
) {
    var toggleFocused by remember { mutableStateOf(false) }

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier.padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = "NOW PLAYING",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(
                text = title ?: "Pick a station",
                style = MaterialTheme.typography.headlineSmall,
            )
            Text(
                text = track ?: status,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(4.dp))
            Button(
                onClick = onToggle,
                enabled = canPlay,
                modifier = Modifier
                    .onFocusChanged { toggleFocused = it.isFocused }
                    .then(
                        if (toggleFocused) {
                            Modifier.border(
                                2.dp,
                                MaterialTheme.colorScheme.primary,
                                RoundedCornerShape(20.dp),
                            )
                        } else {
                            Modifier
                        }
                    ),
            ) {
                Icon(
                    imageVector = if (isPlaying) Icons.Default.Pause else Icons.Default.PlayArrow,
                    contentDescription = null,
                )
                Spacer(Modifier.width(8.dp))
                Text(if (isPlaying) "Pause" else "Play")
            }
        }
    }
}

@Composable
private fun StationRow(
    station: Station,
    isCurrent: Boolean,
    onPlay: () -> Unit,
    onDelete: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    var deleteFocused by remember { mutableStateOf(false) }

    Card(
        modifier = Modifier
            .fillMaxWidth()
            .onFocusChanged { focused = it.isFocused }
            .then(
                if (focused) {
                    Modifier.border(
                        2.dp,
                        MaterialTheme.colorScheme.primary,
                        RoundedCornerShape(12.dp),
                    )
                } else {
                    Modifier
                }
            )
            .clickable(onClick = onPlay),
        colors = if (isCurrent) {
            CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.secondaryContainer,
            )
        } else {
            CardDefaults.cardColors()
        },
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = station.title,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = station.url,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            IconButton(
                onClick = onDelete,
                modifier = Modifier
                    .onFocusChanged { deleteFocused = it.isFocused }
                    .then(
                        if (deleteFocused) {
                            Modifier.border(
                                2.dp,
                                MaterialTheme.colorScheme.primary,
                                CircleShape,
                            )
                        } else {
                            Modifier
                        }
                    ),
            ) {
                Icon(Icons.Default.Delete, contentDescription = "Delete ${station.title}")
            }
        }
    }
}

@Composable
private fun AddStationDialog(
    onDismiss: () -> Unit,
    onAdd: (Station) -> Unit,
) {
    var title by rememberSaveable { mutableStateOf("") }
    var url by rememberSaveable { mutableStateOf("") }
    var showErrors by rememberSaveable { mutableStateOf(false) }

    val validUri = Station.playableUri(url)
    val titleValid = title.isNotBlank()
    val urlValid = validUri != null

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add station") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    label = { Text("Name") },
                    singleLine = true,
                    isError = showErrors && !titleValid,
                )
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text("Stream URL") },
                    singleLine = true,
                    isError = showErrors && !urlValid,
                    supportingText = {
                        if (showErrors && !urlValid) {
                            Text("Enter an http:// or https:// stream URL.")
                        }
                    },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    showErrors = true
                    if (titleValid && validUri != null) {
                        onAdd(
                            Station(
                                title = title.trim(),
                                url = url.trim(),
                            )
                        )
                    }
                },
            ) {
                Text("Add")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        },
    )
}
