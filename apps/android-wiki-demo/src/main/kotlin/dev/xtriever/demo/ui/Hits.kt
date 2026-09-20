package dev.xtriever.demo.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.xtriever.android.Hit
import dev.xtriever.demo.ChangeMark
import dev.xtriever.demo.HitText

/** One hit: its rank, what moved, the article title, the passage and the engine's scores. */
@Composable
fun HitRow(rank: Int, hit: Hit, mark: ChangeMark?, onOpen: (Hit) -> Unit) {
    val (title, passage) = HitText.split(hit.text)
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 4.dp),
    ) {
        Column(Modifier.padding(12.dp)) {
            Row {
                Text("$rank.", style = MaterialTheme.typography.labelLarge)
                Spacer(Modifier.width(8.dp))
                Text(markLabel(mark), style = MaterialTheme.typography.labelLarge)
                Spacer(Modifier.width(8.dp))
                Text(title ?: hit.externalId, style = MaterialTheme.typography.titleSmall)
            }
            Text(
                passage,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 4.dp),
            )
            Text(
                scoreLine(hit),
                style = MaterialTheme.typography.labelSmall,
                modifier = Modifier
                    .padding(top = 6.dp)
                    .fillMaxWidth(),
            )
            Text(
                "explain",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier
                    .padding(top = 4.dp)
                    .clickable { onOpen(hit) },
            )
        }
    }
}

/** The engine's numbers, never the app's. */
private fun scoreLine(hit: Hit): String {
    val rerank = hit.rerankScore?.let { "  rerank %.4f".format(it) } ?: ""
    return "fused %.6f%s".format(hit.score, rerank)
}

private fun markLabel(mark: ChangeMark?): String = when (mark) {
    null -> ""
    ChangeMark.New -> "new"
    ChangeMark.Unchanged -> "="
    is ChangeMark.Up -> "↑${mark.places}"
    is ChangeMark.Down -> "↓${mark.places}"
}
