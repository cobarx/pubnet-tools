package com.cobarx.pubnetchk

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * `@Serializable` mirror of the `pubnetchk` report JSON — the same schema
 * `pubnetchk --json` emits and the HTML report consumes. Only the fields the
 * skeleton UI reads are modelled; unknown keys are ignored so new report fields
 * do not break parsing.
 *
 * Casing note (matches the Rust `serde` attributes in `crates/pubnetchk/src/types.rs`
 * and `crates/pubnet-platform/src/types.rs`):
 *   - most objects are `camelCase`
 *   - `status` / `severity` / `verdict` enums are lowercase
 *   - `score.level` is `Low` / `Medium` / `High` (PascalCase — no rename on the Rust enum)
 *   - `encryption` is `WPA3` / `WPA2` / `WPA2-Enterprise` / `WPA` / `Open` / `Unknown`
 *   - `captivePortal.method` is kebab-case (`content-mismatch`)
 */
object ReportJson {
    val decoder = Json {
        ignoreUnknownKeys = true
        coerceInputValues = true
    }
}

@Serializable
data class Report(
    val version: String,
    val timestamp: String,
    val security: CheckResult<SecurityData>,
    val speed: CheckResult<SpeedData>,
    val reliability: CheckResult<ReliabilityData>,
    val topology: CheckResult<TopologyData>,
    val score: ScoreResult,
)

@Serializable
data class CheckResult<T>(
    val name: String,
    val status: String,
    val data: T? = null,
    val errors: List<String> = emptyList(),
    val findings: List<Finding> = emptyList(),
    val durationMs: Long = 0,
)

@Serializable
data class Finding(
    val id: String,
    val severity: String,
    val points: Int,
    val title: String,
    val detail: String? = null,
)

@Serializable
data class ScoreResult(
    val total: Int,
    val level: String,
    val findings: List<Finding> = emptyList(),
)

// --- topology ---

@Serializable
data class TopologyData(
    // `interface` is a Kotlin hard keyword — map the JSON key explicitly.
    @SerialName("interface") val iface: String? = null,
    val interfaceKind: String? = null,
    val ipCidr: String? = null,
    val gateway: String? = null,
    val neighbors: List<ArpNeighbor> = emptyList(),
    val passiveNotice: String? = null,
)

@Serializable
data class ArpNeighbor(
    val ip: String,
    val mac: String? = null,
    val state: String? = null,
    val device: String? = null,
    val isGateway: Boolean = false,
    val vendor: String? = null,
)

// --- security ---

@Serializable
data class SecurityData(
    val ssid: String? = null,
    val encryption: String? = null,
    val channel: Int? = null,
    val frequencyMhz: Int? = null,
    val signalPercent: Int? = null,
    val dns: DnsResolverInfo? = null,
    val dnsLeak: DnsLeakResult? = null,
    val captivePortal: CaptivePortalResult? = null,
)

@Serializable
data class DnsResolverInfo(
    val link: String? = null,
    val currentServer: String? = null,
    val servers: List<String> = emptyList(),
    val source: String? = null,
)

@Serializable
data class DnsLeakResult(
    val systemEgressIp: String? = null,
    val probes: List<DohProbe> = emptyList(),
    val leaked: Boolean = false,
    val verdict: String? = null,
)

@Serializable
data class DohProbe(
    val provider: String,
    val egressIp: String? = null,
    val reachable: Boolean = false,
)

@Serializable
data class CaptivePortalResult(
    val detected: Boolean = false,
    val method: String? = null,
    val redirectLocation: String? = null,
    val canaryUrl: String? = null,
    val httpStatus: Int? = null,
)

// --- speed / reliability (not run on Android yet; modelled for completeness) ---

@Serializable
data class SpeedData(
    val downloadMbps: Double = 0.0,
    val uploadMbps: Double = 0.0,
    val latencyMs: Double = 0.0,
    val jitterMs: Double = 0.0,
    val source: String = "",
)

@Serializable
data class ReliabilityData(
    val targets: List<ReliabilityTarget> = emptyList(),
    val gatewayReachable: Boolean = false,
    val internetReachable: Boolean = false,
)

@Serializable
data class ReliabilityTarget(
    val host: String,
    val label: String,
    val packetLossPct: Double = 0.0,
    val avgMs: Double? = null,
    val reachable: Boolean = false,
)
