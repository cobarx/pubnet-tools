package com.cobarx.pubnetchk

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel

@Composable
fun MainScreen(
    onScan: () -> Unit,
    onFixWifiName: (WifiNameStatus) -> Unit = {},
    viewModel: AuditViewModel = viewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    Surface(color = MaterialTheme.colorScheme.background) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("Pubnet Tools", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
            Text(
                "Audit the Wi-Fi / network this phone just joined.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Button(
                onClick = onScan,
                enabled = state !is AuditUiState.Running,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(if (state is AuditUiState.Running) "Scanning…" else "Scan")
            }

            when (val s = state) {
                AuditUiState.Idle -> Unit
                AuditUiState.Running -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(vertical = 4.dp),
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(22.dp),
                        strokeWidth = 2.dp,
                    )
                    Spacer(Modifier.width(14.dp))
                    Column {
                        Text(
                            "Running checks…",
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium,
                        )
                        Text(
                            "The speed test takes about 25 seconds.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                is AuditUiState.Error -> ErrorCard(s.message)
                is AuditUiState.Done -> ReportView(s.report, s.wifiName, onFixWifiName)
            }
        }
    }
}

@Composable
private fun ErrorCard(message: String) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text("Could not complete the audit", fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            Text(message, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
private fun ReportView(
    report: Report,
    wifiName: WifiNameStatus,
    onFixWifiName: (WifiNameStatus) -> Unit,
) {
    RiskBadge(report.score.level, report.score.total)

    SectionCard("Network") {
        val t = report.topology.data
        if (report.topology.status == "skipped" || t == null) {
            Text(report.topology.errors.firstOrNull() ?: "No topology data.")
        } else {
            KeyValue("Interface", listOfNotNull(t.iface, t.interfaceKind?.let { "($it)" }).joinToString(" "))
            KeyValue("IP / CIDR", t.ipCidr ?: "—")
            KeyValue("Gateway", t.gateway ?: "—")
            KeyValue("Neighbors", if (t.neighbors.isEmpty()) "none visible" else "${t.neighbors.size}")
        }
        StatusLine(report.topology.status, report.topology.errors)
    }

    SectionCard("Security") {
        val sec = report.security.data
        if (sec == null) {
            Text(report.security.errors.firstOrNull() ?: "No security data.")
        } else {
            if (sec.ssid != null) {
                KeyValue("Wi-Fi", sec.ssid)
            } else {
                WifiNameRow(wifiName, onFixWifiName)
            }
            KeyValue("Encryption", sec.encryption ?: "Unknown")
            KeyValue("DNS servers", sec.dns?.servers?.joinToString(", ").orEmpty().ifBlank { "—" })
            val leak = sec.dnsLeak
            if (leak != null) {
                KeyValue("DNS leak verdict", leak.verdict ?: "—")
                leak.probes.forEach { p ->
                    KeyValue("  DoH ${p.provider}", if (p.reachable) "reachable" else "blocked/unreachable")
                }
            }
            val cp = sec.captivePortal
            if (cp != null) {
                KeyValue("Captive portal", if (cp.detected) "detected (${cp.method})" else "none")
            }
        }
        StatusLine(report.security.status, report.security.errors)
    }

    SectionCard("Performance") {
        val rel = report.reliability.data
        if (report.reliability.status == "skipped" || rel == null) {
            Text(
                report.reliability.errors.firstOrNull() ?: "Reliability was not run.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            KeyValue("Gateway", if (rel.gatewayReachable) "reachable" else "unreachable")
            KeyValue("Internet", if (rel.internetReachable) "reachable" else "unreachable")
            rel.targets.forEach { t ->
                val rtt = t.avgMs?.let { "%.0f ms".format(it) } ?: "—"
                val loss = if (t.packetLossPct > 0) "  ·  %.0f%% loss".format(t.packetLossPct) else ""
                KeyValue("  ${t.host}", if (t.reachable) "$rtt$loss" else "no reply")
            }
        }
        StatusLine(report.reliability.status, report.reliability.errors)

        Spacer(Modifier.height(8.dp))
        val spd = report.speed.data
        if (report.speed.status == "skipped" || spd == null) {
            Text(
                "Speed test is not on Android yet.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            KeyValue("Download", "%.1f Mbps".format(spd.downloadMbps))
            KeyValue("Upload", "%.1f Mbps".format(spd.uploadMbps))
            KeyValue("Latency", "%.0f ms  ·  %.0f ms jitter".format(spd.latencyMs, spd.jitterMs))
        }
    }

    if (report.score.findings.isNotEmpty()) {
        SectionCard("Findings") {
            report.score.findings.sortedByDescending { it.points }.forEach { f ->
                Column(Modifier.padding(vertical = 4.dp)) {
                    Text("• ${f.title}  (${f.severity}, +${f.points})", fontWeight = FontWeight.Medium)
                    f.detail?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                }
            }
        }
    }

    Text(
        "engine ${report.version} · ${report.timestamp}",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun RiskBadge(level: String, total: Int) {
    val (bg, label) = when (level.lowercase()) {
        "low" -> Color(0xFF1B5E20) to "LOW RISK"
        "medium" -> Color(0xFFE65100) to "MEDIUM RISK"
        "high" -> Color(0xFFB71C1C) to "HIGH RISK"
        else -> MaterialTheme.colorScheme.surfaceVariant to level.uppercase()
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = bg),
        shape = RoundedCornerShape(12.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(label, color = Color.White, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.titleLarge)
            Text("score $total", color = Color.White.copy(alpha = 0.85f))
        }
    }
}

@Composable
private fun SectionCard(title: String, content: @Composable () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(8.dp))
            content()
        }
    }
}

@Composable
private fun WifiNameRow(status: WifiNameStatus, onFix: (WifiNameStatus) -> Unit) {
    val (text, action) = when (status) {
        WifiNameStatus.NO_PERMISSION ->
            "name hidden — location permission not granted" to "Grant location permission"
        WifiNameStatus.LOCATION_OFF ->
            "name hidden — system Location is off" to "Open Location settings"
        WifiNameStatus.HIDDEN_OR_UNAVAILABLE ->
            "name unavailable (hidden SSID, or withheld by the OS)" to null
        WifiNameStatus.NOT_WIFI -> "not on Wi-Fi" to null
        WifiNameStatus.VISIBLE -> "" to null // unreachable: caller shows the SSID
    }
    Column(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(
            "Wi-Fi: $text",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (action != null) {
            TextButton(onClick = { onFix(status) }, contentPadding = PaddingValues(0.dp)) {
                Text(action)
            }
        }
    }
}

@Composable
private fun KeyValue(key: String, value: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
        Text(
            key,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(end = 12.dp),
        )
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            fontFamily = FontFamily.Monospace,
            textAlign = TextAlign.End,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun StatusLine(status: String, errors: List<String>) {
    if (status == "ok") return
    Spacer(Modifier.height(6.dp))
    Text(
        buildString {
            append("status: ").append(status)
            if (errors.isNotEmpty()) append(" — ").append(errors.joinToString("; "))
        },
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error,
    )
}
