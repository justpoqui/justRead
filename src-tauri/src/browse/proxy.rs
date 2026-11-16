// Copyright 2026 The justRead Authors
// SPDX-License-Identifier: Apache-2.0

//! The justBrowse network gate: a tiny local HTTP proxy that the browser
//! webview is configured to use for **every** request (pages, images,
//! scripts, XHR …), so the rules below apply to all traffic, not just to
//! top-level navigations:
//!
//! * hosts on the blocklist are refused (trackers, ad networks);
//! * HTTPS (`CONNECT host:443`) is tunnelled **without** decryption — the
//!   proxy only ever sees the host name, never page contents;
//! * plain HTTP is refused unless the user allowed that host after the
//!   "not encrypted" warning (HTTPS-only by default);
//! * destinations that resolve to this machine (loopback, unspecified,
//!   link-local) are refused, after DNS resolution, so a web page can't
//!   reach local services or this proxy — including via DNS rebinding;
//! * destinations on the local network (private ranges such as 192.168.x.x,
//!   10.x, IPv6 unique-local) are refused too, so a web page can't attack the
//!   router, printers or other devices at home. A host the user explicitly
//!   allowed (the "not encrypted" prompt, e.g. a Calibre server on the LAN)
//!   may still be reached.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::blocklist::Blocklist;

/// Largest request head we accept (request line + headers).
const MAX_HEAD: usize = 32 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Shared, live configuration of the proxy.
pub struct Gate {
    pub blocklist: Blocklist,
    /// Hosts the user allowed over plain HTTP (this session only).
    pub http_allowed: Mutex<HashSet<String>>,
    /// Number of requests refused by the blocklist.
    pub blocked: AtomicU64,
    /// Development/testing escape hatch for local test servers.
    pub allow_loopback: bool,
    /// Called (with the host) whenever a request is blocked.
    pub on_block: Box<dyn Fn(&str) + Send + Sync>,
}

impl Gate {
    pub fn new(allow_loopback: bool, on_block: Box<dyn Fn(&str) + Send + Sync>) -> Self {
        Self {
            blocklist: Blocklist::bundled(),
            http_allowed: Mutex::new(HashSet::new()),
            blocked: AtomicU64::new(0),
            allow_loopback,
            on_block,
        }
    }

    fn http_ok(&self, host: &str) -> bool {
        self.http_allowed
            .lock()
            .map(|s| s.contains(&host.to_ascii_lowercase()))
            .unwrap_or(false)
    }
}

/// Bind to an ephemeral loopback port and serve forever. Returns the address.
pub async fn start(gate: Arc<Gate>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((stream, peer)) = listener.accept().await else {
                continue;
            };
            // Only this machine may use the proxy (it is bound to loopback anyway).
            if !peer.ip().is_loopback() {
                continue;
            }
            let gate = gate.clone();
            tokio::spawn(async move {
                let _ = handle(stream, gate).await;
            });
        }
    });
    Ok(addr)
}

/// Handle one client connection (one request, or one CONNECT tunnel).
async fn handle(mut client: TcpStream, gate: Arc<Gate>) -> std::io::Result<()> {
    // STEP 1: read the request head.
    let mut buf = Vec::with_capacity(4096);
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = find(&buf, b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > MAX_HEAD {
            return reply(&mut client, 431, "Request header too large").await;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let rest = buf[head_end..].to_vec();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let (method, target, version) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or("HTTP/1.1"),
    );

    if method.eq_ignore_ascii_case("CONNECT") {
        // STEP 2a: HTTPS tunnel. Only standard TLS ports are tunnelled.
        let (host, port) = split_host_port(target, 443);
        // (Test mode with loopback allowed also allows any port, for local servers.)
        if let Some(code) = refuse(
            &gate,
            &host,
            port == 443 || port == 8443 || gate.allow_loopback,
            true,
        ) {
            return reply(&mut client, code, "Blocked by justBrowse").await;
        }
        let Some(mut upstream) = connect(&gate, &host, port).await else {
            return reply(&mut client, 502, "Could not reach the site").await;
        };
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        if !rest.is_empty() {
            upstream.write_all(&rest).await?;
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
        return Ok(());
    }

    // STEP 2b: plain HTTP — absolute-form request ("GET http://host/path HTTP/1.1").
    let Some(after_scheme) = target.strip_prefix("http://") else {
        return reply(&mut client, 400, "Bad request").await;
    };
    let (authority, path) = match after_scheme.find('/') {
        Some(i) => (&after_scheme[..i], &after_scheme[i..]),
        None => (after_scheme, "/"),
    };
    let (host, port) = split_host_port(authority, 80);
    if let Some(code) = refuse(&gate, &host, true, false) {
        return reply(&mut client, code, "Blocked by justBrowse").await;
    }
    if !gate.http_ok(&host) {
        return reply(
            &mut client,
            403,
            "justBrowse is HTTPS-only: this site was not allowed over HTTP",
        )
        .await;
    }
    let Some(mut upstream) = connect(&gate, &host, port).await else {
        return reply(&mut client, 502, "Could not reach the site").await;
    };

    // STEP 3: forward with an origin-form request line, without proxy headers,
    // and one request per connection (simple and safe).
    let mut out = format!("{method} {path} {version}\r\n");
    for line in lines.filter(|l| !l.is_empty()) {
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if name.starts_with("proxy-") || name == "connection" || name == "keep-alive" {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    upstream.write_all(out.as_bytes()).await?;
    upstream.write_all(&rest).await?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

/// Status code to refuse with, if the request must not go through.
fn refuse(gate: &Gate, host: &str, port_ok: bool, _tls: bool) -> Option<u16> {
    if host.is_empty() || !port_ok {
        return Some(403);
    }
    if gate.blocklist.is_blocked(host) {
        gate.blocked.fetch_add(1, Ordering::Relaxed);
        (gate.on_block)(host);
        return Some(403);
    }
    if !gate.allow_loopback && is_local_name(host) {
        return Some(403);
    }
    // An address literal on the local network: only if the user allowed it.
    if !gate.allow_loopback
        && host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(is_private_ip)
        && !gate.http_ok(host)
    {
        return Some(403);
    }
    None
}

/// Resolve and connect, refusing local destinations (DNS-rebinding safe).
async fn connect(gate: &Gate, host: &str, port: u16) -> Option<TcpStream> {
    let addrs = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .ok()?
        .ok()?;
    // A name that resolves into the home network needs the user's explicit allowance.
    let lan_ok = gate.allow_loopback || gate.http_ok(host);
    for addr in addrs {
        if !gate.allow_loopback && is_local_ip(addr.ip()) {
            continue;
        }
        if !lan_ok && is_private_ip(addr.ip()) {
            continue;
        }
        if let Ok(Ok(s)) = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
            return Some(s);
        }
    }
    None
}

fn is_local_name(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "localhost"
        || h.ends_with(".localhost")
        || h.parse::<IpAddr>().map(is_local_ip).unwrap_or(false)
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback() || v.is_unspecified() || v.is_link_local(),
        IpAddr::V6(v) => {
            v.is_loopback()
                || v.is_unspecified()
                || v.to_ipv4_mapped()
                    .is_some_and(|m| m.is_loopback() || m.is_unspecified() || m.is_link_local())
        }
    }
}

/// Addresses on a local or special-purpose network (not the public internet).
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_private()                                  // 10/8, 172.16/12, 192.168/16
                || (o[0] == 100 && (o[1] & 0xc0) == 64)     // 100.64/10 carrier-grade NAT
                || (o[0] == 198 && (o[1] & 0xfe) == 18)     // 198.18/15 benchmarking
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)  // 192.0.0/24 protocol assignments
                || o[0] == 0                                // 0/8 "this network"
                || v.is_multicast()
                || v.is_broadcast()
                || o[0] >= 240                              // 240/4 reserved
                || is_local_ip(ip)
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            (s[0] & 0xfe00) == 0xfc00                       // fc00::/7 unique local
                || (s[0] & 0xffc0) == 0xfe80                // fe80::/10 link-local
                || (s[0] & 0xffc0) == 0xfec0                // fec0::/10 old site-local
                || v.is_multicast()
                || is_local_ip(ip)
                // IPv4 inside IPv6 (mapped ::ffff:a.b.c.d, NAT64 64:ff9b::/96).
                || v.to_ipv4_mapped().is_some_and(|m| is_private_ip(IpAddr::V4(m)))
                || (s[0..6] == [0x64, 0xff9b, 0, 0, 0, 0] && {
                    let [a, b] = s[6].to_be_bytes();
                    let [c, d] = s[7].to_be_bytes();
                    is_private_ip(IpAddr::V4(std::net::Ipv4Addr::new(a, b, c, d)))
                })
        }
    }
}

/// "host:port" / "[v6]:port" / "host" → (host, port).
fn split_host_port(authority: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = authority.strip_prefix('[') {
        let (h, tail) = rest.split_once(']').unwrap_or((rest, ""));
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse().ok())
            .unwrap_or(default_port);
        return (h.to_string(), port);
    }
    match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h.to_string(), p.parse().unwrap_or(default_port))
        }
        _ => (authority.to_string(), default_port),
    }
}

async fn reply(client: &mut TcpStream, code: u16, text: &str) -> std::io::Result<()> {
    let reason = match code {
        400 => "Bad Request",
        403 => "Forbidden",
        431 => "Request Header Fields Too Large",
        _ => "Bad Gateway",
    };
    let body = format!("{text}\n");
    let resp = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    client.write_all(resp.as_bytes()).await?;
    client.shutdown().await
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-shot upstream server that answers every connection with `answer`.
    async fn upstream(answer: &'static [u8]) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                let mut b = [0u8; 4096];
                let _ = s.read(&mut b).await;
                let _ = s.write_all(answer).await;
            }
        });
        a
    }

    async fn ask(proxy: SocketAddr, req: &str) -> String {
        let mut s = TcpStream::connect(proxy).await.unwrap();
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await;
        String::from_utf8_lossy(&out).into_owned()
    }

    fn gate(loopback: bool) -> Arc<Gate> {
        Arc::new(Gate::new(loopback, Box::new(|_| {})))
    }

    #[tokio::test]
    async fn blocks_trackers_and_counts_them() {
        let g = gate(false);
        let p = start(g.clone()).await.unwrap();
        let r = ask(
            p,
            "CONNECT stats.g.doubleclick.net:443 HTTP/1.1\r\nHost: x\r\n\r\n",
        )
        .await;
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        assert_eq!(g.blocked.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn refuses_local_destinations_and_odd_ports() {
        let p = start(gate(false)).await.unwrap();
        for req in [
            "CONNECT localhost:443 HTTP/1.1\r\n\r\n",
            "CONNECT 127.0.0.1:443 HTTP/1.1\r\n\r\n",
            "CONNECT [::1]:443 HTTP/1.1\r\n\r\n",
            "CONNECT 169.254.169.254:443 HTTP/1.1\r\n\r\n",
            "CONNECT example.org:22 HTTP/1.1\r\n\r\n",
            // The home network: router, NAS, printers.
            "CONNECT 192.168.1.1:443 HTTP/1.1\r\n\r\n",
            "CONNECT 10.0.0.1:443 HTTP/1.1\r\n\r\n",
            "CONNECT [fd00::1]:443 HTTP/1.1\r\n\r\n",
            "CONNECT [::ffff:192.168.0.1]:443 HTTP/1.1\r\n\r\n",
        ] {
            assert!(ask(p, req).await.starts_with("HTTP/1.1 403"), "{req}");
        }
    }

    #[test]
    fn classifies_private_addresses() {
        let private = |s: &str| is_private_ip(s.parse().unwrap());
        for a in [
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.10",
            "100.64.0.1",
            "169.254.1.1",
            "127.0.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "fd12::1",
            "fe80::1",
            "::1",
            "::ffff:10.0.0.1",
            "64:ff9b::c0a8:0101",
            "ff02::1",
        ] {
            assert!(private(a), "{a} should be private");
        }
        for a in [
            "93.184.216.34",
            "172.32.0.1",
            "100.128.0.1",
            "8.8.8.8",
            "2606:4700::1111",
            "64:ff9b::808:808",
        ] {
            assert!(!private(a), "{a} should be public");
        }
    }

    #[tokio::test]
    async fn plain_http_needs_permission() {
        // STEP 1: an upstream on loopback (allowed only in this test).
        let up = upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
        let g = gate(true);
        let p = start(g.clone()).await.unwrap();
        let req = format!("GET http://127.0.0.1:{}/page HTTP/1.1\r\nHost: 127.0.0.1\r\nProxy-Connection: keep-alive\r\n\r\n", up.port());
        // STEP 2: refused until the host is allowed, then forwarded.
        assert!(ask(p, &req).await.contains("HTTPS-only"));
        g.http_allowed.lock().unwrap().insert("127.0.0.1".into());
        assert!(ask(p, &req).await.ends_with("ok"));
    }

    #[tokio::test]
    async fn tunnels_https_without_looking_inside() {
        // STEP 1: CONNECT to an upstream (loopback allowed in test mode).
        let up = upstream(b"opaque-tls-bytes").await;
        let p = start(gate(true)).await.unwrap();
        let mut s = TcpStream::connect(p).await.unwrap();
        s.write_all(format!("CONNECT 127.0.0.1:{} HTTP/1.1\r\n\r\n", up.port()).as_bytes())
            .await
            .unwrap();
        // STEP 2: tunnel established, then bytes flow both ways untouched.
        let mut b = [0u8; 39];
        s.read_exact(&mut b).await.unwrap();
        assert_eq!(&b, b"HTTP/1.1 200 Connection Established\r\n\r\n");
        s.write_all(b"client-hello").await.unwrap();
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out)).await;
        assert_eq!(out, b"opaque-tls-bytes");

        // Outside test mode, non-TLS ports are refused.
        let strict = start(gate(false)).await.unwrap();
        assert!(ask(strict, "CONNECT example.org:8080 HTTP/1.1\r\n\r\n")
            .await
            .starts_with("HTTP/1.1 403"));
        assert_eq!(
            split_host_port("example.org:8443", 443),
            ("example.org".into(), 8443)
        );
        assert_eq!(
            split_host_port("[2001:db8::1]:443", 1),
            ("2001:db8::1".into(), 443)
        );
        assert_eq!(
            split_host_port("example.org", 443),
            ("example.org".into(), 443)
        );
    }
}
