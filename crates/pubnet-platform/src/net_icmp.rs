//! Unprivileged ICMP echo over a datagram socket (`SOCK_DGRAM` / `IPPROTO_ICMP`).
//!
//! Linux and Android let a normal process open this socket type when its gid is
//! within `/proc/sys/net/ipv4/ping_group_range` — Android ships that range as
//! `0 2147483647` (every app), so no `CAP_NET_RAW`, no root, no `ping` binary.
//! This is how the reliability check pings on Android, where it cannot shell out
//! to `/system/bin/ping`. See
//! `docs/decisions/2026-09-02-android-unprivileged-icmp.md`.
//!
//! IPv4 only (the reliability targets are `1.1.1.1` / `8.8.8.8` / the v4
//! gateway). The kernel rewrites the ICMP `id` to the socket and matches replies
//! to it, so pairing is by `seq`; the kernel also fills the checksum for
//! `SOCK_DGRAM`, but we compute it too so the code is not Linux-quirk-dependent.

#![cfg(any(target_os = "linux", target_os = "android"))]

use crate::network::PingSummary;
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::mem::MaybeUninit;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;
const PER_PACKET_TIMEOUT: Duration = Duration::from_millis(1000);
const INTER_PACKET_GAP: Duration = Duration::from_millis(200);
const PAYLOAD: &[u8] = b"pubnetchk";

/// Ping `host` `count` times. Mirrors `platform::windows::icmp_ping`: a bad /
/// non-v4 host yields `transmitted = count, received = 0`; the work runs on the
/// blocking pool (one synchronous socket, sequential echoes with a 200 ms gap).
pub async fn icmp_ping(host: &str, count: u32) -> PingSummary {
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return PingSummary {
            transmitted: count,
            received: 0,
            rtts: Vec::new(),
        };
    };
    tokio::task::spawn_blocking(move || ping_blocking(ip, count))
        .await
        .unwrap_or(PingSummary {
            transmitted: count,
            received: 0,
            rtts: Vec::new(),
        })
}

fn ping_blocking(ip: Ipv4Addr, count: u32) -> PingSummary {
    let socket = match open_socket() {
        Ok(s) => s,
        // No socket -> report the target as fully lost, like a spawn failure on
        // the shell-out path. The check turns this into `reachable: false`.
        Err(_) => {
            return PingSummary {
                transmitted: count,
                received: 0,
                rtts: Vec::new(),
            };
        }
    };
    let dest = SocketAddrV4::new(ip, 0);

    let mut rtts = Vec::new();
    let mut received = 0;
    for seq in 0..count as u16 {
        if seq > 0 {
            std::thread::sleep(INTER_PACKET_GAP);
        }
        let packet = echo_request(seq);
        let sent_at = Instant::now();
        if socket.send_to(&packet, &dest.into()).is_err() {
            continue;
        }
        if let Some(rtt) = await_reply(&socket, seq, sent_at) {
            received += 1;
            rtts.push(rtt);
        }
    }

    PingSummary {
        transmitted: count,
        received,
        rtts,
    }
}

fn open_socket() -> io::Result<Socket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))?;
    socket.set_read_timeout(Some(PER_PACKET_TIMEOUT))?;
    Ok(socket)
}

/// One reply matching `seq` (an Echo Reply, or an ICMP error carrying our
/// request), or `None` on timeout / unrelated traffic within the window.
fn await_reply(socket: &Socket, seq: u16, sent_at: Instant) -> Option<f64> {
    let mut buf = [MaybeUninit::<u8>::uninit(); 1500];
    loop {
        if sent_at.elapsed() >= PER_PACKET_TIMEOUT {
            return None;
        }
        let n = socket.recv(&mut buf).ok()?;
        let data = unsafe { slice_assume_init(&buf[..n]) };
        // A datagram ICMP socket delivers the ICMP message with no IP header.
        if data.len() < 8 {
            continue;
        }
        match data[0] {
            ECHO_REPLY if u16::from_be_bytes([data[6], data[7]]) == seq => {
                return Some(sent_at.elapsed().as_secs_f64() * 1000.0);
            }
            // Destination Unreachable / Time Exceeded etc. embed the original
            // request after an 8-byte ICMP header + 20-byte IP header. If the
            // embedded ICMP seq is ours, the target/path rejected this probe —
            // count it as lost (return None) rather than waiting out the timeout.
            3 | 11 if embedded_seq(data) == Some(seq) => return None,
            _ => continue,
        }
    }
}

fn embedded_seq(icmp_error: &[u8]) -> Option<u16> {
    // 8 (icmp err hdr) + 20 (inner IP hdr, no options) + 8 (inner icmp hdr)
    let inner = icmp_error.get(8..)?;
    let inner_icmp = inner.get(20..28)?;
    Some(u16::from_be_bytes([inner_icmp[6], inner_icmp[7]]))
}

fn echo_request(seq: u16) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(8 + PAYLOAD.len());
    pkt.extend_from_slice(&[ECHO_REQUEST, 0, 0, 0]); // type, code, checksum[0..2]
    pkt.extend_from_slice(&0u16.to_be_bytes()); // id (kernel overwrites)
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(PAYLOAD);
    let ck = checksum(&pkt);
    pkt[2..4].copy_from_slice(&ck.to_be_bytes());
    pkt
}

/// Standard internet checksum (RFC 1071) over the ICMP message.
fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for pair in data.chunks(2) {
        let word = match pair {
            [hi, lo] => u16::from_be_bytes([*hi, *lo]),
            [hi] => u16::from_be_bytes([*hi, 0]),
            _ => 0,
        };
        sum += u32::from(word);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

unsafe fn slice_assume_init(s: &[MaybeUninit<u8>]) -> &[u8] {
    unsafe { &*(s as *const [MaybeUninit<u8>] as *const [u8]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_a_known_vector() {
        // Echo request, id 0, seq 0, payload "pubnetchk", checksum field zeroed.
        let mut pkt = vec![8u8, 0, 0, 0, 0, 0, 0, 0];
        pkt.extend_from_slice(b"pubnetchk");
        let ck = checksum(&pkt);
        pkt[2..4].copy_from_slice(&ck.to_be_bytes());
        // Checksum over the whole packet (incl. the filled field) is 0.
        assert_eq!(checksum(&pkt), 0);
    }

    #[test]
    fn non_ipv4_host_is_all_loss() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let s = rt.block_on(icmp_ping("not-an-ip", 3));
        assert_eq!(s.transmitted, 3);
        assert_eq!(s.received, 0);
        assert!(s.rtts.is_empty());
    }

    // Real boundary: a datagram ICMP socket to a public anycast address. Skipped
    // automatically where the socket can't be opened (ping_group_range) or the
    // network blocks ICMP — asserts shape, not exact RTT.
    // spec: reliability-check-resilience
    #[test]
    fn pings_a_public_address_when_icmp_is_permitted() {
        if open_socket().is_err() {
            eprintln!("skipped: unprivileged ICMP not permitted here");
            return;
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let s = rt.block_on(icmp_ping("1.1.1.1", 4));
        assert_eq!(s.transmitted, 4);
        assert!(s.received <= 4);
        assert_eq!(s.rtts.len() as u32, s.received);
        for rtt in &s.rtts {
            assert!(*rtt >= 0.0 && *rtt < 1000.0);
        }
    }
}
