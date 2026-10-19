//! Caller address resolution. Forwarding headers are read only when the direct
//! peer is a trusted proxy, and proxy hops are walked from the nearest one back.
use http::{HeaderMap, HeaderName};
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

/// Loopback, private and link-local networks: a reverse proxy on the host or a container network.
const DEFAULT_PROXIES: &str =
    "127.0.0.0/8,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,169.254.0.0/16,::1/128,fc00::/7,fe80::/10";
const DEFAULT_HEADERS: &str = "x-forwarded-for,x-real-ip";

static POLICY: OnceLock<ClientIpPolicy> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
struct Network {
    v4: bool,
    address: u128,
    mask: u128,
}
impl Network {
    fn parse(text: &str) -> Result<Self, String> {
        let invalid = || format!("Invalid trusted proxy network: {text}");
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (text, None),
        };
        let address = canonical(address.parse::<IpAddr>().map_err(|_| invalid())?);
        let (v4, bits, width) = match address {
            IpAddr::V4(v4) => (true, u128::from(u32::from(v4)), 32),
            IpAddr::V6(v6) => (false, u128::from(v6), 128),
        };
        let prefix = match prefix {
            Some(prefix) => prefix
                .parse::<u32>()
                .ok()
                .filter(|p| *p <= width)
                .ok_or_else(invalid)?,
            None => width,
        };
        let mask = match prefix {
            0 => 0,
            _ => (u128::MAX << (width - prefix)) & (u128::MAX >> (128 - width)),
        };
        Ok(Self {
            v4,
            address: bits & mask,
            mask,
        })
    }
    fn contains(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => self.v4 && u128::from(u32::from(v4)) & self.mask == self.address,
            IpAddr::V6(v6) => !self.v4 && u128::from(v6) & self.mask == self.address,
        }
    }
}

/// IPv4-mapped IPv6 addresses are the IPv4 caller.
fn canonical(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

/// One forwarded address: a bare IP, `ip:port` or `[ipv6]:port`.
fn parse(value: &str) -> Option<IpAddr> {
    let value = value.trim().trim_matches('"');
    value
        .parse::<IpAddr>()
        .ok()
        .or_else(|| value.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
        .or_else(|| value.strip_prefix('[')?.strip_suffix(']')?.parse().ok())
        .map(canonical)
}

#[derive(Clone, Debug)]
pub struct ClientIpPolicy {
    proxies: Vec<Network>,
    headers: Vec<HeaderName>,
}

impl Default for ClientIpPolicy {
    fn default() -> Self {
        Self::new(DEFAULT_PROXIES, DEFAULT_HEADERS).expect("default client IP policy")
    }
}

impl ClientIpPolicy {
    pub fn new(proxies: &str, headers: &str) -> Result<Self, String> {
        let proxies = proxies
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(Network::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let headers = headers
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|name| {
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| format!("Invalid client IP header: {name}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { proxies, headers })
    }
    /// `CODEX2API_TRUSTED_PROXIES` (IPs or CIDRs; `none` trusts no proxy) and
    /// `CODEX2API_CLIENT_IP_HEADERS` (ordered); unset or empty values use the defaults.
    pub fn from_env() -> Result<Self, String> {
        let setting = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| default.into())
        };
        let proxies = setting("CODEX2API_TRUSTED_PROXIES", DEFAULT_PROXIES);
        let proxies = if proxies.trim().eq_ignore_ascii_case("none") {
            ""
        } else {
            &proxies
        };
        Self::new(
            proxies,
            &setting("CODEX2API_CLIENT_IP_HEADERS", DEFAULT_HEADERS),
        )
    }
    /// Install the process-wide policy before serving requests.
    pub fn install(self) -> bool {
        POLICY.set(self).is_ok()
    }
    fn trusted(&self, ip: IpAddr) -> bool {
        self.proxies.iter().any(|network| network.contains(ip))
    }
    pub fn resolve(&self, peer: Option<SocketAddr>, headers: &HeaderMap) -> Option<IpAddr> {
        let peer = canonical(peer?.ip());
        if !self.trusted(peer) {
            return Some(peer);
        }
        'headers: for name in &self.headers {
            // Proxies append hops; every header instance is one ordered list.
            let mut hops = headers
                .get_all(name)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|v| v.split(','))
                .filter(|v| !v.trim().is_empty())
                .rev()
                .peekable();
            if hops.peek().is_none() {
                continue;
            }
            let mut farthest = None;
            for hop in hops {
                let Some(ip) = parse(hop) else {
                    continue 'headers;
                };
                if !self.trusted(ip) {
                    return Some(ip);
                }
                farthest = Some(ip);
            }
            if farthest.is_some() {
                return farthest;
            }
        }
        Some(peer)
    }
}

/// The request's client address under the installed policy.
pub fn client_ip(peer: Option<SocketAddr>, headers: &HeaderMap) -> Option<IpAddr> {
    POLICY.get_or_init(Default::default).resolve(peer, headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }
    fn peer(ip: &str) -> Option<SocketAddr> {
        Some(SocketAddr::new(ip.parse().unwrap(), 443))
    }
    #[test]
    fn forwarded_headers_need_a_trusted_peer_and_walk_from_the_nearest_hop() {
        let policy = ClientIpPolicy::default();
        let spoofed = headers(&[("x-forwarded-for", "1.1.1.1"), ("x-real-ip", "2.2.2.2")]);
        assert_eq!(
            policy.resolve(peer("203.0.113.9"), &spoofed),
            "203.0.113.9".parse().ok()
        );
        let chain = headers(&[
            ("x-forwarded-for", "9.9.9.9, 198.51.100.7"),
            ("x-forwarded-for", "10.0.0.2"),
        ]);
        assert_eq!(
            policy.resolve(peer("127.0.0.1"), &chain),
            "198.51.100.7".parse().ok()
        );
        let internal = headers(&[("x-forwarded-for", "192.168.1.5, 10.0.0.2")]);
        assert_eq!(
            policy.resolve(peer("::1"), &internal),
            "192.168.1.5".parse().ok()
        );
        let invalid = headers(&[
            ("x-forwarded-for", "unknown"),
            ("x-real-ip", "[2001:db8::7]:8443"),
        ]);
        assert_eq!(
            policy.resolve(peer("172.17.0.1"), &invalid),
            "2001:db8::7".parse().ok()
        );
        assert_eq!(
            policy.resolve(peer("::ffff:10.0.0.3"), &HeaderMap::new()),
            "10.0.0.3".parse().ok()
        );
        assert_eq!(policy.resolve(None, &chain), None);
    }
    #[test]
    fn configured_headers_and_networks_replace_the_defaults() {
        let policy =
            ClientIpPolicy::new("203.0.113.0/24", "cf-connecting-ip,x-forwarded-for").unwrap();
        let cdn = headers(&[
            ("cf-connecting-ip", "198.51.100.20:5000"),
            ("x-forwarded-for", "8.8.8.8"),
        ]);
        assert_eq!(
            policy.resolve(peer("203.0.113.40"), &cdn),
            "198.51.100.20".parse().ok()
        );
        assert_eq!(
            policy.resolve(peer("127.0.0.1"), &cdn),
            "127.0.0.1".parse().ok()
        );
        assert!(
            ClientIpPolicy::new("", DEFAULT_HEADERS)
                .unwrap()
                .proxies
                .is_empty()
        );
        assert!(ClientIpPolicy::new("10.0.0.0/33", DEFAULT_HEADERS).is_err());
        assert!(ClientIpPolicy::new("example.com", DEFAULT_HEADERS).is_err());
    }
}
