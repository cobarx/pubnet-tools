package com.cobarx.pubnetchk

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.LinkAddress
import android.net.LinkProperties
import android.net.NetworkCapabilities
import android.net.RouteInfo
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import androidx.core.content.ContextCompat
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.File
import java.net.Inet4Address

/**
 * Gathers the `HostSnapshot` the Rust engine's `SnapshotProbe` is fed — the
 * Android equivalent of shelling out to `ip` / `nmcli` / `resolvectl`, which an
 * app cannot do.
 *
 * Field contract: `docs/specs/android-host-snapshot.md`. The JSON is camelCase
 * and every sub-object is nullable — a fact we cannot read is left `null` and
 * the checks downstream already tolerate the corresponding `None`.
 */
object NetworkFacts {
    private const val TAG = "NetworkFacts"

    private val json = Json { encodeDefaults = true; explicitNulls = true }

    fun collect(context: Context): HostSnapshot {
        val cm = context.getSystemService(ConnectivityManager::class.java)
            ?: return HostSnapshot() // no ConnectivityManager -> empty snapshot; topology skips
        val network = cm.activeNetwork
            ?: return HostSnapshot() // airplane mode / no network -> defaultRoute stays null (spec S4)
        val caps = cm.getNetworkCapabilities(network)
        val link = cm.getLinkProperties(network)

        val iface = link?.interfaceName
        val kind = interfaceKind(caps)

        val defaultRoute = link?.let { defaultRoute(it) }
        // Kotlin sees the gateway before the ARP scan; the Rust side re-checks it.
        val gatewayIp = defaultRoute?.gateway

        return HostSnapshot(
            defaultRoute = defaultRoute,
            interfaceAddr = link?.let { interfaceAddr(it) },
            arpNeighbors = readArpTable(iface, gatewayIp),
            wifi = if (kind == "wifi") wifiFacts(context, cm, network) else null,
            dns = link?.let { dnsFacts(it) },
            interfaceKind = kind,
        )
    }

    fun toJson(snapshot: HostSnapshot): String = json.encodeToString(HostSnapshot.serializer(), snapshot)

    // --- routes / addresses ---

    private fun defaultRoute(link: LinkProperties): SnapshotRoute? {
        val route: RouteInfo = link.routes.firstOrNull { it.isDefaultRoute && it.gateway != null }
            ?: return null
        val gw = route.gateway ?: return null
        return SnapshotRoute(gateway = gw.hostAddress ?: return null, device = link.interfaceName ?: "")
    }

    private fun interfaceAddr(link: LinkProperties): SnapshotAddr? {
        // Prefer the IPv4 link address; the engine's topology section is IPv4-CIDR shaped.
        val addr: LinkAddress = link.linkAddresses.firstOrNull { it.address is Inet4Address }
            ?: link.linkAddresses.firstOrNull()
            ?: return null
        val host = addr.address.hostAddress ?: return null
        return SnapshotAddr(ip = host, prefix = addr.prefixLength)
    }

    private fun interfaceKind(caps: NetworkCapabilities?): String = when {
        caps == null -> "other"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
        caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
        else -> "other"
    }

    // --- DNS ---

    private fun dnsFacts(link: LinkProperties): SnapshotDns {
        val servers = link.dnsServers.mapNotNull { it.hostAddress }
        return SnapshotDns(servers = servers, currentServer = servers.firstOrNull())
    }

    // --- Wi-Fi ---

    private fun wifiFacts(
        context: Context,
        cm: ConnectivityManager,
        network: android.net.Network,
    ): SnapshotWifi {
        val hasLocation = ContextCompat.checkSelfPermission(
            context, Manifest.permission.ACCESS_FINE_LOCATION,
        ) == PackageManager.PERMISSION_GRANTED

        val info: WifiInfo? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (cm.getNetworkCapabilities(network)?.transportInfo as? WifiInfo)
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(WifiManager::class.java)?.connectionInfo
        }

        // <unknown ssid> is Android's placeholder when the SSID is withheld.
        val rawSsid = info?.ssid?.trim('"')
        val ssidUsable = hasLocation && !rawSsid.isNullOrBlank() &&
            rawSsid != WifiManager.UNKNOWN_SSID && rawSsid != "0x"

        // Linear dBm -> percent (the instance `calculateSignalLevel` is API 30+
        // and returns coarse buckets; the static overload is deprecated).
        val signalPercent = info?.rssi?.let { rssi ->
            when {
                rssi <= -100 -> 0
                rssi >= -50 -> 100
                else -> 2 * (rssi + 100)
            }
        }
        val freq = info?.frequency?.takeIf { it > 0 }

        return SnapshotWifi(
            ssid = if (ssidUsable) rawSsid else null,
            ssidHidden = !ssidUsable,
            encryption = encryptionOf(info),
            channel = freq?.let { channelForFrequency(it) },
            frequencyMhz = freq,
            signalPercent = signalPercent,
        )
    }

    /** `WifiInfo.getCurrentSecurityType()` is API 31+; older devices report `Unknown`. */
    private fun encryptionOf(info: WifiInfo?): String {
        if (info == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return "Unknown"
        return when (info.currentSecurityType) {
            WifiInfo.SECURITY_TYPE_OPEN, WifiInfo.SECURITY_TYPE_OWE -> "Open"
            WifiInfo.SECURITY_TYPE_WEP -> "WPA"
            WifiInfo.SECURITY_TYPE_PSK -> "WPA2"
            WifiInfo.SECURITY_TYPE_SAE -> "WPA3"
            WifiInfo.SECURITY_TYPE_EAP,
            WifiInfo.SECURITY_TYPE_EAP_WPA3_ENTERPRISE,
            WifiInfo.SECURITY_TYPE_EAP_WPA3_ENTERPRISE_192_BIT -> "WPA2-Enterprise"
            else -> "Unknown"
        }
    }

    private fun channelForFrequency(mhz: Int): Int? = when {
        mhz == 2484 -> 14
        mhz in 2412..2472 -> (mhz - 2407) / 5
        mhz in 5160..5885 -> (mhz - 5000) / 5
        mhz in 5955..7115 -> (mhz - 5950) / 5 // 6 GHz
        else -> null
    }

    // --- ARP ---

    /**
     * `/proc/net/arp` is unreadable to apps on most Android 10+ builds; when it
     * is, return `[]` and topology still reports `ok` (a present address is what
     * makes it `ok` — neighbors are additive). See spec S5.
     */
    private fun readArpTable(iface: String?, gatewayIp: String?): List<SnapshotNeighbor> {
        val file = File("/proc/net/arp")
        if (!file.canRead()) return emptyList()
        return try {
            file.readLines()
                .drop(1) // header row
                .mapNotNull { line ->
                    val cols = line.trim().split(Regex("\\s+"))
                    if (cols.size < 4) return@mapNotNull null
                    val ip = cols[0]
                    val mac = cols[3].takeIf { it != "00:00:00:00:00:00" }
                    // cols[5] is the device on most builds
                    val dev = cols.getOrNull(5)
                    if (iface != null && dev != null && dev != iface) return@mapNotNull null
                    SnapshotNeighbor(ip = ip, mac = mac, isGateway = ip == gatewayIp)
                }
        } catch (e: Exception) {
            Log.d(TAG, "unable to read /proc/net/arp: ${e.message}")
            emptyList()
        }
    }
}

// --- HostSnapshot wire model (camelCase JSON — docs/specs/android-host-snapshot.md) ---

@Serializable
data class HostSnapshot(
    val defaultRoute: SnapshotRoute? = null,
    val interfaceAddr: SnapshotAddr? = null,
    val arpNeighbors: List<SnapshotNeighbor> = emptyList(),
    val wifi: SnapshotWifi? = null,
    val dns: SnapshotDns? = null,
    val interfaceKind: String = "other",
)

@Serializable
data class SnapshotRoute(val gateway: String, val device: String)

@Serializable
data class SnapshotAddr(val ip: String, val prefix: Int)

@Serializable
data class SnapshotNeighbor(
    val ip: String,
    val mac: String? = null,
    val isGateway: Boolean = false,
)

@Serializable
data class SnapshotWifi(
    val ssid: String? = null,
    val ssidHidden: Boolean = false,
    val encryption: String = "Unknown",
    val channel: Int? = null,
    val frequencyMhz: Int? = null,
    val signalPercent: Int? = null,
)

@Serializable
data class SnapshotDns(
    val servers: List<String> = emptyList(),
    val currentServer: String? = null,
)

/** Options object for `runAuditJson` — camelCase, mirrors `AndroidOptions` in the Rust crate. */
@Serializable
data class AndroidOptions(
    val only: List<String> = listOf("topology", "security"),
    @SerialName("speedDurationSecs") val speedDurationSecs: Long = 10,
    @SerialName("wifiDetail") val wifiDetail: Boolean = true,
)
