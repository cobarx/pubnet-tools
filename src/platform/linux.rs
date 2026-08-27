//! Linux implementation of PlatformProbe.
//! Commands: ip, nmcli, resolvectl.

use super::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo, is_vpn_iface};
use crate::exec::{ExecResult, cmd, exec_cmd};
use crate::network::{
    extract_remote_ip, parse_ip_addr, parse_ip_neigh, parse_ip_route, parse_nmcli_wifi,
    parse_resolvectl_status,
};
use crate::types::{ArpNeighbor, DnsResolverInfo, InterfaceKind};

fn empty() -> ExecResult {
    ExecResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    }
}

pub struct LinuxProbe;

impl PlatformProbe for LinuxProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        let r = exec_cmd(cmd(&["ip", "route", "show", "default"]))
            .await
            .ok()?;
        let route = parse_ip_route(&r.stdout)?;
        Some(RouteInfo {
            gateway: route.gateway,
            device: route.device,
        })
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let r = exec_cmd(cmd(&["ip", "addr", "show", iface])).await.ok()?;
        let addr = parse_ip_addr(&r.stdout)?;
        Some(AddrInfo {
            ip: addr.ip,
            prefix: addr.prefix,
        })
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        let r = exec_cmd(cmd(&["ip", "neigh", "show", "dev", iface]))
            .await
            .unwrap_or_else(|_| empty());
        parse_ip_neigh(&r.stdout, iface, gateway_ip)
    }

    async fn wifi_info(&self) -> Option<WifiInfo> {
        let r = exec_cmd(cmd(&[
            "nmcli",
            "-t",
            "-f",
            "active,ssid,security,chan,freq,signal",
            "dev",
            "wifi",
            "list",
        ]))
        .await
        .ok()?;
        let w = parse_nmcli_wifi(&r.stdout)?;
        Some(WifiInfo {
            ssid: w.ssid,
            encryption: w.encryption,
            channel: w.channel,
            frequency_mhz: w.frequency_mhz,
            signal_percent: w.signal_percent,
        })
    }

    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo> {
        let r = exec_cmd(cmd(&["resolvectl", "status"])).await.ok()?;
        parse_resolvectl_status(&r.stdout, iface)
    }

    async fn system_egress_ip(&self) -> Option<String> {
        let r = exec_cmd(cmd(&[
            "resolvectl",
            "query",
            "--type=TXT",
            "whoami.cloudflare.com",
        ]))
        .await
        .ok()?;
        extract_remote_ip(&r.stdout)
    }

    async fn interface_type(&self, iface: &str) -> InterfaceKind {
        if is_vpn_iface(iface) {
            return InterfaceKind::Vpn;
        }
        // /sys/class/net/<iface>/wireless exists only for WiFi interfaces
        let wifi_path = format!("/sys/class/net/{iface}/wireless");
        if tokio::fs::try_exists(&wifi_path).await.unwrap_or(false) {
            return InterfaceKind::WiFi;
        }
        InterfaceKind::Ethernet
    }
}
