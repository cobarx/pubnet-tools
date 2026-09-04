package com.cobarx.pubnetchk

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.pubnetchk_android.reportSchemaVersion

/**
 * Skeleton screen for epic ticket 4: it does nothing but call across the UniFFI
 * bridge, so a successful launch proves the JNI load + binding path work. The
 * real Scan screen is ticket 5.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    Text(
                        "pubnetchk engine ${reportSchemaVersion()}",
                        modifier = Modifier.padding(24.dp),
                        style = MaterialTheme.typography.titleMedium,
                    )
                }
            }
        }
    }
}
