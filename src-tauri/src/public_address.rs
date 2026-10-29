use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
};

/// Whether an address may be contacted for an archive link from an untrusted source.
pub(crate) fn is_public(address: IpAddr) -> bool {
    // Unit tests serve fixtures from loopback.
    (cfg!(test) && address.is_loopback()) || is_routable(address)
}

fn is_routable(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_routable_v4(address),
        IpAddr::V6(address) => is_routable_v6(address),
    }
}

fn is_routable_v4(address: Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();
    let blocked = first == 0
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_broadcast()
        || address.is_documentation()
        // Shared address space (CGNAT), 100.64.0.0/10.
        || (first == 100 && (second & 0xc0) == 64)
        // IETF protocol assignments, 192.0.0.0/24.
        || (first == 192 && second == 0 && third == 0)
        // Benchmarking, 198.18.0.0/15.
        || (first == 198 && (second & 0xfe) == 18)
        // Reserved, 240.0.0.0/4.
        || first >= 240;
    !blocked
}

fn is_routable_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    if address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || address.is_unique_local()
        || address.is_unicast_link_local()
        // Deprecated site-local, fec0::/10.
        || (segments[0] & 0xffc0) == 0xfec0
        // Documentation, 2001:db8::/32.
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
    {
        return false;
    }
    // IPv4-mapped, IPv4-compatible and NAT64 (64:ff9b::/96) addresses reach the embedded IPv4 host.
    let nat64 = segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0];
    match address.to_ipv4() {
        Some(embedded) => is_routable_v4(embedded),
        None if nat64 => {
            let [.., high, low] = segments;
            is_routable_v4(Ipv4Addr::from((u32::from(high) << 16) | u32::from(low)))
        }
        None => true,
    }
}

/// Whether a URL names a host that may be contacted. IP literals skip DNS, so they are
/// checked here; names are checked again by `PublicResolver` after resolution.
pub(crate) fn is_public_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(address) = host.parse::<IpAddr>() {
        return is_public(address);
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host != "localhost" && !host.ends_with(".localhost")
}

/// Resolves host names and keeps only public addresses.
pub(crate) struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(public_addresses(name.as_str().to_owned()))
    }
}

async fn public_addresses(host: String) -> Result<Addrs, Box<dyn Error + Send + Sync>> {
    let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), 0))
        .await?
        .filter(|address| is_public(address.ip()))
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host} does not resolve to a public address").into());
    }
    let addresses: Addrs = Box::new(addresses.into_iter());
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routable(address: &str) -> bool {
        is_routable(address.parse().unwrap())
    }

    #[test]
    fn rejects_non_public_ipv4() {
        for address in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            assert!(!routable(address), "{address}");
        }
        for address in ["1.1.1.1", "8.8.8.8", "100.128.0.1", "172.32.0.1"] {
            assert!(routable(address), "{address}");
        }
    }

    #[test]
    fn rejects_non_public_ipv6() {
        for address in [
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "2001:db8::1",
        ] {
            assert!(!routable(address), "{address}");
        }
        assert!(routable("2606:4700:4700::1111"));
    }

    #[test]
    fn checks_ipv4_embedded_in_ipv6() {
        for address in [
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::192.168.0.1",
            "64:ff9b::a00:1",
        ] {
            assert!(!routable(address), "{address}");
        }
        assert!(routable("::ffff:8.8.8.8"));
        assert!(routable("64:ff9b::808:808"));
    }

    #[test]
    fn rejects_local_hosts_in_urls() {
        for url in [
            "http://localhost/file.zip",
            "http://LOCALHOST./file.zip",
            "http://files.localhost/file.zip",
            "http://192.168.0.10/file.zip",
            "http://[fe80::1]/file.zip",
            "http://[::ffff:10.0.0.1]/file.zip",
            // Decimal form of 10.0.0.1.
            "http://167772161/file.zip",
        ] {
            assert!(!is_public_host(&Url::parse(url).unwrap()), "{url}");
        }
        for url in [
            "https://example.com/file.zip",
            "https://notlocalhost.com/file.zip",
            "https://8.8.8.8/file.zip",
        ] {
            assert!(is_public_host(&Url::parse(url).unwrap()), "{url}");
        }
    }
}
