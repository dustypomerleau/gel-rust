use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::Arc,
};

use derive_more::Debug;
use rustls_pki_types::ServerName;

use crate::TlsParameters;

#[derive(Clone)]
/// A target name describes the TCP or Unix socket that a client will connect to.
pub struct TargetName {
    inner: MaybeResolvedTarget,
}

impl std::fmt::Debug for TargetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl TargetName {
    /// Create a new target for a Unix socket.
    pub fn new_unix_path(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        #[cfg(unix)]
        {
            let path = ResolvedTarget::from(std::os::unix::net::SocketAddr::from_pathname(path)?);
            Ok(Self {
                inner: MaybeResolvedTarget::Resolved(path),
            })
        }
        #[cfg(not(unix))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Unix sockets are not supported on this platform",
            ))
        }
    }

    /// Create a new target for a Unix socket.
    pub fn new_unix_domain(domain: impl AsRef<[u8]>) -> Result<Self, std::io::Error> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            use std::os::linux::net::SocketAddrExt;
            let domain =
                ResolvedTarget::from(std::os::unix::net::SocketAddr::from_abstract_name(domain)?);
            Ok(Self {
                inner: MaybeResolvedTarget::Resolved(domain),
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Unix domain sockets are not supported on this platform",
            ))
        }
    }

    /// Create a new target for a TCP socket.
    #[allow(private_bounds)]
    pub fn new_tcp(host: impl TcpResolve) -> Self {
        Self { inner: host.into() }
    }

    /// Resolves the target addresses for a given host.
    pub fn to_addrs_sync(&self) -> Result<Vec<ResolvedTarget>, std::io::Error> {
        use std::net::ToSocketAddrs;
        let mut result = Vec::new();
        match &self.inner {
            MaybeResolvedTarget::Resolved(addr) => {
                return Ok(vec![addr.clone()]);
            }
            MaybeResolvedTarget::Unresolved(host, port, _interface) => {
                let addrs = format!("{host}:{port}").to_socket_addrs()?;
                result.extend(addrs.map(ResolvedTarget::SocketAddr));
            }
        }
        Ok(result)
    }

    pub(crate) fn maybe_resolved(&self) -> &MaybeResolvedTarget {
        &self.inner
    }

    pub(crate) fn maybe_resolved_mut(&mut self) -> &mut MaybeResolvedTarget {
        &mut self.inner
    }

    /// Check if the target is a TCP connection.
    pub fn is_tcp(&self) -> bool {
        self.maybe_resolved().port().is_some()
    }

    /// Get the port of the target. If the target type does not include a port,
    /// this will return None.
    pub fn port(&self) -> Option<u16> {
        self.maybe_resolved().port()
    }

    /// Set the port of the target. If the target type does not include a port,
    /// this will return None. Otherwise, it will return the old port.
    pub fn try_set_port(&mut self, port: u16) -> Option<u16> {
        self.maybe_resolved_mut().set_port(port)
    }

    /// Get the path of the target. If the target type does not include a path,
    /// this will return None.
    pub fn path(&self) -> Option<&Path> {
        self.maybe_resolved().path()
    }

    /// Get the host of the target. For resolved IP addresses, this is the
    /// string representation of the IP address. For unresolved hostnames, this
    /// is the hostname. If the target type does not include a host, this will
    /// return None.
    pub fn host(&self) -> Option<Cow<str>> {
        self.maybe_resolved().host()
    }

    /// Get the name of the target. For resolved IP addresses, this is the
    /// string representation of the IP address. For unresolved hostnames, this
    /// is the hostname.
    pub fn name(&self) -> Option<ServerName> {
        self.maybe_resolved().name()
    }

    /// Get the host and port of the target. If the target type does not include
    /// a host or port, this will return None.
    pub fn tcp(&self) -> Option<(Cow<str>, u16)> {
        self.maybe_resolved().tcp()
    }
}

/// A target describes the TCP or Unix socket that a client will connect to,
/// along with any optional TLS parameters.
#[derive(Clone)]
pub struct Target {
    inner: TargetInner,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            TargetInner::NoTls(target) => write!(f, "{target:?}"),
            TargetInner::Tls(target, _) => write!(f, "{target:?} (TLS)"),
            TargetInner::StartTls(target, _) => write!(f, "{target:?} (STARTTLS)"),
        }
    }
}

#[allow(private_bounds)]
impl Target {
    pub fn new(name: TargetName) -> Self {
        Self {
            inner: TargetInner::NoTls(name.inner),
        }
    }

    pub fn new_tls(name: TargetName, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::Tls(name.inner, params.into()),
        }
    }

    pub fn new_starttls(name: TargetName, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::StartTls(name.inner, params.into()),
        }
    }

    pub fn new_resolved(target: ResolvedTarget) -> Self {
        Self {
            inner: TargetInner::NoTls(target.into()),
        }
    }

    pub fn new_resolved_tls(target: ResolvedTarget, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::Tls(target.into(), params.into()),
        }
    }

    pub fn new_resolved_starttls(target: ResolvedTarget, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::StartTls(target.into(), params.into()),
        }
    }

    /// Create a new target for a Unix socket.
    pub fn new_unix_path(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        #[cfg(unix)]
        {
            let path = ResolvedTarget::from(std::os::unix::net::SocketAddr::from_pathname(path)?);
            Ok(Self {
                inner: TargetInner::NoTls(path.into()),
            })
        }
        #[cfg(not(unix))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Unix sockets are not supported on this platform",
            ))
        }
    }

    /// Create a new target for a Unix socket.
    pub fn new_unix_domain(domain: impl AsRef<[u8]>) -> Result<Self, std::io::Error> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            use std::os::linux::net::SocketAddrExt;
            let domain =
                ResolvedTarget::from(std::os::unix::net::SocketAddr::from_abstract_name(domain)?);
            Ok(Self {
                inner: TargetInner::NoTls(domain.into()),
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Unix domain sockets are not supported on this platform",
            ))
        }
    }

    /// Create a new target for a TCP socket.
    pub fn new_tcp(host: impl TcpResolve) -> Self {
        Self {
            inner: TargetInner::NoTls(host.into()),
        }
    }

    /// Create a new target for a TCP socket with TLS.
    pub fn new_tcp_tls(host: impl TcpResolve, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::Tls(host.into(), params.into()),
        }
    }

    /// Create a new target for a TCP socket with STARTTLS.
    pub fn new_tcp_starttls(host: impl TcpResolve, params: TlsParameters) -> Self {
        Self {
            inner: TargetInner::StartTls(host.into(), params.into()),
        }
    }

    pub fn try_set_tls(&mut self, params: TlsParameters) -> Option<Option<Arc<TlsParameters>>> {
        // Don't set TLS parameters on Unix sockets.
        if self.maybe_resolved().path().is_some() {
            return None;
        }

        let params = params.into();

        // Temporary
        let no_target = TargetInner::NoTls(MaybeResolvedTarget::Resolved(
            ResolvedTarget::SocketAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
        ));

        match std::mem::replace(&mut self.inner, no_target) {
            TargetInner::NoTls(target) => {
                self.inner = TargetInner::Tls(target, params);
                Some(None)
            }
            TargetInner::Tls(target, old_params) => {
                self.inner = TargetInner::Tls(target, params);
                Some(Some(old_params))
            }
            TargetInner::StartTls(target, old_params) => {
                self.inner = TargetInner::StartTls(target, params);
                Some(Some(old_params))
            }
        }
    }

    pub fn try_remove_tls(&mut self) -> Option<Arc<TlsParameters>> {
        // Temporary
        let no_target = TargetInner::NoTls(MaybeResolvedTarget::Resolved(
            ResolvedTarget::SocketAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
        ));

        match std::mem::replace(&mut self.inner, no_target) {
            TargetInner::NoTls(target) => {
                self.inner = TargetInner::NoTls(target);
                None
            }
            TargetInner::Tls(target, old_params) => {
                self.inner = TargetInner::NoTls(target);
                Some(old_params)
            }
            TargetInner::StartTls(target, old_params) => {
                self.inner = TargetInner::NoTls(target);
                Some(old_params)
            }
        }
    }

    /// Check if the target is a TCP connection.
    pub fn is_tcp(&self) -> bool {
        self.maybe_resolved().port().is_some()
    }

    /// Get the port of the target. If the target type does not include a port,
    /// this will return None.
    pub fn port(&self) -> Option<u16> {
        self.maybe_resolved().port()
    }

    /// Set the port of the target. If the target type does not include a port,
    /// this will return None. Otherwise, it will return the old port.
    pub fn try_set_port(&mut self, port: u16) -> Option<u16> {
        self.maybe_resolved_mut().set_port(port)
    }

    /// Get the path of the target. If the target type does not include a path,
    /// this will return None.
    pub fn path(&self) -> Option<&Path> {
        self.maybe_resolved().path()
    }

    /// Get the host of the target. For resolved IP addresses, this is the
    /// string representation of the IP address. For unresolved hostnames, this
    /// is the hostname. If the target type does not include a host, this will
    /// return None.
    pub fn host(&self) -> Option<Cow<str>> {
        self.maybe_resolved().host()
    }

    /// Get the name of the target. For resolved IP addresses, this is the
    /// string representation of the IP address. For unresolved hostnames, this
    /// is the hostname.
    pub fn name(&self) -> Option<ServerName> {
        self.maybe_resolved().name()
    }

    /// Get the host and port of the target. If the target type does not include
    /// a host or port, this will return None.
    pub fn tcp(&self) -> Option<(Cow<str>, u16)> {
        self.maybe_resolved().tcp()
    }

    pub(crate) fn maybe_resolved(&self) -> &MaybeResolvedTarget {
        match &self.inner {
            TargetInner::NoTls(target) => target,
            TargetInner::Tls(target, _) => target,
            TargetInner::StartTls(target, _) => target,
        }
    }

    pub(crate) fn maybe_resolved_mut(&mut self) -> &mut MaybeResolvedTarget {
        match &mut self.inner {
            TargetInner::NoTls(target) => target,
            TargetInner::Tls(target, _) => target,
            TargetInner::StartTls(target, _) => target,
        }
    }

    pub(crate) fn is_starttls(&self) -> bool {
        matches!(self.inner, TargetInner::StartTls(_, _))
    }

    pub(crate) fn maybe_ssl(&self) -> Option<&TlsParameters> {
        match &self.inner {
            TargetInner::NoTls(_) => None,
            TargetInner::Tls(_, params) => Some(params),
            TargetInner::StartTls(_, params) => Some(params),
        }
    }
}

#[derive(Clone, derive_more::From)]
pub(crate) enum MaybeResolvedTarget {
    Resolved(ResolvedTarget),
    Unresolved(Cow<'static, str>, u16, Option<Cow<'static, str>>),
}

impl std::fmt::Debug for MaybeResolvedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => {
                if let SocketAddr::V6(addr) = addr {
                    if addr.scope_id() != 0 {
                        write!(f, "[{}%{}]:{}", addr.ip(), addr.scope_id(), addr.port())
                    } else {
                        write!(f, "[{}]:{}", addr.ip(), addr.port())
                    }
                } else {
                    write!(f, "{}:{}", addr.ip(), addr.port())
                }
            }
            #[cfg(unix)]
            MaybeResolvedTarget::Resolved(ResolvedTarget::UnixSocketAddr(addr)) => {
                if let Some(path) = addr.as_pathname() {
                    return write!(f, "{}", path.to_string_lossy());
                } else {
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    {
                        use std::os::linux::net::SocketAddrExt;
                        if let Some(name) = addr.as_abstract_name() {
                            return write!(f, "@{}", String::from_utf8_lossy(name));
                        }
                    }
                }
                Ok(())
            }
            MaybeResolvedTarget::Unresolved(host, port, interface) => {
                write!(f, "{host}:{port}")?;
                if let Some(interface) = interface {
                    write!(f, "%{interface}")?;
                }
                Ok(())
            }
        }
    }
}

impl MaybeResolvedTarget {
    fn name(&self) -> Option<ServerName> {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => {
                Some(ServerName::IpAddress(addr.ip().into()))
            }
            MaybeResolvedTarget::Unresolved(host, _, _) => {
                Some(ServerName::DnsName(host.to_string().try_into().ok()?))
            }
            #[cfg(unix)]
            _ => None,
        }
    }

    fn tcp(&self) -> Option<(Cow<str>, u16)> {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => {
                Some((Cow::Owned(addr.ip().to_string()), addr.port()))
            }
            MaybeResolvedTarget::Unresolved(host, port, _) => Some((Cow::Borrowed(host), *port)),
            #[cfg(unix)]
            _ => None,
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            #[cfg(unix)]
            MaybeResolvedTarget::Resolved(ResolvedTarget::UnixSocketAddr(addr)) => {
                addr.as_pathname()
            }
            _ => None,
        }
    }

    fn host(&self) -> Option<Cow<str>> {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => {
                Some(Cow::Owned(addr.ip().to_string()))
            }
            MaybeResolvedTarget::Unresolved(host, _, _) => Some(Cow::Borrowed(host)),
            #[cfg(unix)]
            _ => None,
        }
    }

    fn port(&self) -> Option<u16> {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => Some(addr.port()),
            MaybeResolvedTarget::Unresolved(_, port, _) => Some(*port),
            #[cfg(unix)]
            _ => None,
        }
    }

    fn set_port(&mut self, new_port: u16) -> Option<u16> {
        match self {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(addr)) => {
                let old_port = addr.port();
                addr.set_port(new_port);
                Some(old_port)
            }
            MaybeResolvedTarget::Unresolved(_, port, _) => {
                let old_port = *port;
                *port = new_port;
                Some(old_port)
            }
            #[cfg(unix)]
            _ => None,
        }
    }
}

/// The type of connection.
#[derive(Clone, Debug)]
enum TargetInner {
    NoTls(MaybeResolvedTarget),
    Tls(MaybeResolvedTarget, Arc<TlsParameters>),
    StartTls(MaybeResolvedTarget, Arc<TlsParameters>),
}

#[derive(Clone, Debug, derive_more::From, derive_more::TryFrom)]
/// The resolved target of a connection attempt.
#[from(forward)]
pub enum ResolvedTarget {
    SocketAddr(std::net::SocketAddr),
    #[cfg(unix)]
    UnixSocketAddr(std::os::unix::net::SocketAddr),
}

/// Because `std::os::unix::net::SocketAddr` does not implement many helper
/// traits, we temporarily use this enum to represent the inner representation
/// of the resolved target for easier operation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ResolvedTargetInner<'a> {
    SocketAddr(std::net::SocketAddr),
    #[cfg(unix)]
    UnixSocketPath(&'a std::path::Path),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    UnixSocketAbstract(&'a [u8]),
    /// Windows doesn't need the lifetime, so we create a fake enum variant
    /// to use it.
    #[allow(dead_code)]
    Phantom(std::marker::PhantomData<&'a ()>),
}

#[cfg(unix)]
impl TryFrom<std::path::PathBuf> for ResolvedTarget {
    type Error = std::io::Error;

    fn try_from(value: std::path::PathBuf) -> Result<Self, Self::Error> {
        Ok(ResolvedTarget::UnixSocketAddr(
            std::os::unix::net::SocketAddr::from_pathname(value)?,
        ))
    }
}

impl Eq for ResolvedTarget {}

impl PartialEq for ResolvedTarget {
    fn eq(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }
}

impl Hash for ResolvedTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner().hash(state);
    }
}

impl PartialOrd for ResolvedTarget {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner().partial_cmp(&other.inner())
    }
}

impl ResolvedTarget {
    pub fn tcp(&self) -> Option<SocketAddr> {
        match self {
            ResolvedTarget::SocketAddr(addr) => Some(*addr),
            _ => None,
        }
    }

    pub fn is_tcp(&self) -> bool {
        self.tcp().is_some()
    }

    pub fn transport(&self) -> Transport {
        match self {
            ResolvedTarget::SocketAddr(_) => Transport::Tcp,
            #[cfg(unix)]
            ResolvedTarget::UnixSocketAddr(_) => Transport::Unix,
        }
    }

    /// Get the inner representation of the resolved target.
    #[allow(unreachable_code)]
    fn inner(&self) -> ResolvedTargetInner {
        match self {
            ResolvedTarget::SocketAddr(addr) => ResolvedTargetInner::SocketAddr(*addr),
            #[cfg(unix)]
            ResolvedTarget::UnixSocketAddr(addr) => {
                if let Some(path) = addr.as_pathname() {
                    return ResolvedTargetInner::UnixSocketPath(path);
                } else {
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    {
                        use std::os::linux::net::SocketAddrExt;
                        return ResolvedTargetInner::UnixSocketAbstract(
                            addr.as_abstract_name().expect("abstract socket address"),
                        );
                    }
                }
                unreachable!()
            }
        }
    }
}

/// A trait for types that have a local address.
pub trait LocalAddress {
    fn local_address(&self) -> std::io::Result<ResolvedTarget>;
}

/// A trait for types that have a local address.
pub trait RemoteAddress {
    fn remote_address(&self) -> std::io::Result<ResolvedTarget>;
}

pub trait PeerCred {
    #[cfg(all(unix, feature = "tokio"))]
    fn peer_cred(&self) -> std::io::Result<tokio::net::unix::UCred>;
}

/// The transport of a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    Tcp,
    Unix,
}

/// A trait for stream metadata.
pub trait StreamMetadata: LocalAddress + RemoteAddress + PeerCred + Send {
    fn transport(&self) -> Transport;
}

pub(crate) trait TcpResolve {
    fn into(self) -> MaybeResolvedTarget;
}

impl<S: AsRef<str>> TcpResolve for (S, u16) {
    fn into(self) -> MaybeResolvedTarget {
        if let Ok(addr) = self.0.as_ref().parse::<IpAddr>() {
            MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(SocketAddr::new(addr, self.1)))
        } else {
            MaybeResolvedTarget::Unresolved(Cow::Owned(self.0.as_ref().to_owned()), self.1, None)
        }
    }
}

impl TcpResolve for SocketAddr {
    fn into(self) -> MaybeResolvedTarget {
        MaybeResolvedTarget::Resolved(ResolvedTarget::SocketAddr(self))
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV6;

    use super::*;

    #[test]
    fn test_target() {
        let target = Target::new_tcp(("localhost", 5432));
        assert_eq!(
            target.name(),
            Some(ServerName::DnsName("localhost".try_into().unwrap()))
        );
    }

    #[test]
    fn test_target_name() {
        let target = TargetName::new_tcp(("localhost", 5432));
        assert_eq!(format!("{target:?}"), "localhost:5432");

        let target = TargetName::new_tcp(("127.0.0.1", 5432));
        assert_eq!(format!("{target:?}"), "127.0.0.1:5432");

        let target = TargetName::new_tcp(("::1", 5432));
        assert_eq!(format!("{target:?}"), "[::1]:5432");

        let target = TargetName::new_tcp(SocketAddr::V6(SocketAddrV6::new(
            "fe80::1ff:fe23:4567:890a".parse().unwrap(),
            5432,
            0,
            2,
        )));
        assert_eq!(format!("{target:?}"), "[fe80::1ff:fe23:4567:890a%2]:5432");

        #[cfg(unix)]
        {
            let target = TargetName::new_unix_path("/tmp/test.sock").unwrap();
            assert_eq!(format!("{target:?}"), "/tmp/test.sock");
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let target = TargetName::new_unix_domain("test").unwrap();
            assert_eq!(format!("{target:?}"), "@test");
        }
    }

    #[test]
    fn test_target_debug() {
        let target = Target::new_tcp(("localhost", 5432));
        assert_eq!(format!("{target:?}"), "localhost:5432");

        let target = Target::new_tcp_tls(("localhost", 5432), TlsParameters::default());
        assert_eq!(format!("{target:?}"), "localhost:5432 (TLS)");

        let target = Target::new_tcp_starttls(("localhost", 5432), TlsParameters::default());
        assert_eq!(format!("{target:?}"), "localhost:5432 (STARTTLS)");

        let target = Target::new_tcp(("127.0.0.1", 5432));
        assert_eq!(format!("{target:?}"), "127.0.0.1:5432");

        let target = Target::new_tcp(("::1", 5432));
        assert_eq!(format!("{target:?}"), "[::1]:5432");

        #[cfg(unix)]
        {
            let target = Target::new_unix_path("/tmp/test.sock").unwrap();
            assert_eq!(format!("{target:?}"), "/tmp/test.sock");
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let target = Target::new_unix_domain("test").unwrap();
            assert_eq!(format!("{target:?}"), "@test");
        }
    }
}
