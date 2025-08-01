use super::{
    error::*, format_duration, BuildContextImpl, CredentialsFile, FromParamStr, InstanceName,
    Param, Params,
};
use crate::{
    gel::{parse_duration, BuildPhase},
    host::{Host, HostType, LOCALHOST_HOSTNAME},
};
use rustls_pki_types::CertificateDer;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    num::NonZero,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use url::Url;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_WAIT: Duration = Duration::from_secs(30);
pub const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(60);
pub const DEFAULT_POOL_SIZE: usize = 10;
pub const DEFAULT_HOST: &HostType = crate::host::LOCALHOST;
pub const DEFAULT_PORT: u16 = 5656;
pub const DEFAULT_USER: &str = crate::gel::branding::BRANDING_DEFAULT_USERNAME_LEGACY;
pub const DEFAULT_BRANCH: DatabaseBranch = DatabaseBranch::Default;

pub const DEFAULT_DATABASE_NAME: &str = "edgedb";

/// The branch name used when connecting to an existing instance to request
/// the default branch.
pub const DEFAULT_BRANCH_NAME_CONNECT: &str = "__default__";
/// The default branch name used when creating a new instance.
pub const DEFAULT_BRANCH_NAME_CREATE: &str = "main";

/// The result of building a [`Config`].
pub struct ConfigResult {
    pub(crate) result: Result<Config, gel_errors::Error>,
    pub(crate) warnings: Warnings,
}

impl std::ops::Deref for ConfigResult {
    type Target = Result<Config, gel_errors::Error>;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

impl From<ConfigResult> for Result<Config, gel_errors::Error> {
    fn from(val: ConfigResult) -> Self {
        val.result
    }
}

impl ConfigResult {
    pub fn unwrap(self) -> Config {
        self.result.unwrap()
    }

    pub fn expect(self, message: &str) -> Config {
        self.result.expect(message)
    }

    pub fn result(&self) -> &Result<Config, gel_errors::Error> {
        &self.result
    }

    pub fn into_result(self) -> Result<Config, gel_errors::Error> {
        self.result
    }

    pub fn parse_error(&self) -> Option<&ParseError> {
        use std::error::Error;
        self.result
            .as_ref()
            .err()
            .and_then(|e| e.source().and_then(|e| e.downcast_ref::<ParseError>()))
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// The configuration for a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub host: Host,
    pub db: DatabaseBranch,
    pub user: String,

    /// If the configuration was loaded from an instance name, this will be present.
    pub instance_name: Option<InstanceName>,

    pub authentication: Authentication,

    pub client_security: ClientSecurity,
    pub tls_security: TlsSecurity,

    pub tls_ca: Option<Vec<CertificateDer<'static>>>,
    pub tls_server_name: Option<String>,
    pub wait_until_available: Duration,

    pub connect_timeout: Duration,
    pub max_concurrency: Option<usize>,
    pub tcp_keepalive: TcpKeepalive,

    pub cloud_certs: Option<CloudCerts>,

    pub server_settings: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: Host::new(DEFAULT_HOST.clone(), DEFAULT_PORT),
            db: DatabaseBranch::Default,
            user: DEFAULT_USER.to_string(),
            instance_name: None,
            authentication: Authentication::None,
            client_security: ClientSecurity::Default,
            tls_security: TlsSecurity::Strict,
            tls_ca: None,
            tls_server_name: None,
            wait_until_available: DEFAULT_WAIT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            max_concurrency: None,
            tcp_keepalive: TcpKeepalive::Default,
            cloud_certs: None,
            server_settings: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Error, derive_more::Display)]
pub enum CredentialsError {
    #[display("no TCP address")]
    NoTcpAddress,
}

fn to_pem(certs: &[CertificateDer<'static>]) -> String {
    use base64::Engine;
    let prefix = "-----BEGIN CERTIFICATE-----\n";
    let suffix = "-----END CERTIFICATE-----\n";
    let mut pem = String::new();
    for cert in certs {
        pem.push_str(prefix);
        let mut b64 = vec![0; cert.len() * 4 / 3 + 4];
        let len = base64::prelude::BASE64_STANDARD
            .encode_slice(cert.as_ref(), &mut b64)
            .unwrap();
        b64.truncate(len);
        let lines = b64.chunks(64);
        for line in lines {
            pem.push_str(std::str::from_utf8(line).unwrap());
            pem.push('\n');
        }
        pem.push_str(suffix);
    }
    pem
}

impl Config {
    pub fn instance_name(&self) -> Option<&InstanceName> {
        self.instance_name.as_ref()
    }

    pub fn local_instance_name(&self) -> Option<&str> {
        self.instance_name.as_ref().and_then(InstanceName::local)
    }

    pub fn admin(&self) -> bool {
        self.host.is_unix()
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn port(&self) -> u16 {
        self.host.1
    }

    pub fn display_addr(&self) -> impl fmt::Display + '_ {
        self.host.to_string()
    }

    pub fn secret_key(&self) -> Option<&str> {
        self.authentication.secret_key()
    }

    pub fn tls_ca_pem(&self) -> Option<String> {
        self.tls_ca.as_ref().map(|v| to_pem(v))
    }

    /// Return HTTP(s) url to server if not connected via unix socket.
    pub fn http_url(&self, tls: bool) -> Option<String> {
        if let Some((host, port)) = self.host.target_name().ok()?.tcp() {
            let s = if tls { "s" } else { "" };
            Some(format!("http{s}://{host}:{port}"))
        } else {
            None
        }
    }

    /// Return DSN url to server if not connected via unix socket.
    ///
    /// Note that this method is not guaranteed to return a fully-connectable URL.
    pub fn dsn_url(&self) -> Option<String> {
        let mut url = Url::parse("gel://").unwrap();

        if let Some((host, port)) = self.host.target_name().ok()?.tcp() {
            if host != LOCALHOST_HOSTNAME {
                if port != DEFAULT_PORT {
                    _ = url.set_host(Some(&host));
                    _ = url.set_port(Some(port));
                } else {
                    _ = url.set_host(Some(&host));
                }
            } else if port != DEFAULT_PORT {
                url.query_pairs_mut().append_pair("port", &port.to_string());
            }
        } else {
            return None;
        }

        if self.db != DatabaseBranch::Default {
            if let Some(database) = self.db.database() {
                url.set_path(database);
            }

            if let Some(branch) = self.db.branch_for_connect() {
                url.set_path(branch);
            }
        }

        if self.user() != DEFAULT_USER {
            if url.host().is_none() {
                url.query_pairs_mut().append_pair("user", self.user());
            } else {
                _ = url.set_username(self.user());
            }
        }

        if let Some(password) = self.authentication.password() {
            if url.host().is_none() {
                url.query_pairs_mut().append_pair("password", password);
            } else {
                _ = url.set_password(Some(password));
            }
        }

        // NOTE: The user will need to provide a CA file
        if self.tls_ca.is_some() {
            url.query_pairs_mut().append_pair("tls_ca_file", "<...>");
        }

        if let Some(secret_key) = self.authentication.secret_key() {
            url.query_pairs_mut().append_pair("secret_key", secret_key);
        }

        if self.tls_security != TlsSecurity::Strict {
            url.query_pairs_mut()
                .append_pair("tls_security", &self.tls_security.to_string());
        }

        if let Some(tls_server_name) = &self.tls_server_name {
            url.query_pairs_mut()
                .append_pair("tls_server_name", tls_server_name);
        }

        if self.wait_until_available != DEFAULT_WAIT {
            url.query_pairs_mut().append_pair(
                "wait_until_available",
                &format_duration(&self.wait_until_available),
            );
        }

        for (key, value) in &self.server_settings {
            url.query_pairs_mut().append_pair(key, value);
        }

        Some(url.to_string())
    }

    pub fn with_host(&self, host: &str, port: u16) -> Result<Self, ParseError> {
        Ok(Self {
            host: Host::new(HostType::from_str(host)?, port),
            ..self.clone()
        })
    }

    pub fn with_branch(&self, branch: &str) -> Self {
        Self {
            db: DatabaseBranch::Branch(branch.to_string()),
            ..self.clone()
        }
    }

    pub fn with_db(&self, db: DatabaseBranch) -> Self {
        Self { db, ..self.clone() }
    }

    pub fn with_user(&self, user: &str) -> Self {
        Self {
            user: user.to_string(),
            ..self.clone()
        }
    }

    pub fn with_password(&self, password: &str) -> Self {
        Self {
            authentication: Authentication::Password(password.to_string()),
            ..self.clone()
        }
    }

    pub fn with_wait_until_available(&self, dur: Duration) -> Self {
        Self {
            wait_until_available: dur,
            ..self.clone()
        }
    }

    pub fn with_tls_ca(&self, certs: &[CertificateDer<'static>]) -> Self {
        Self {
            tls_ca: Some(certs.to_vec()),
            ..self.clone()
        }
    }

    #[deprecated = "use with_tls_ca instead"]
    pub fn with_pem_certificates(&self, certs: &str) -> Result<Self, ParseError> {
        let certs = <Vec<CertificateDer<'static>> as FromParamStr>::from_param_str(
            certs,
            &BuildContextImpl::default(),
        )?;
        Ok(Self {
            tls_ca: Some(certs),
            ..self.clone()
        })
    }

    #[cfg(feature = "serde")]
    pub fn to_json(&self) -> impl serde::Serialize + std::fmt::Display {
        use serde::Serialize;
        use std::collections::BTreeMap;

        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct ConfigJson {
            address: (String, usize),
            branch: Option<String>,
            database: Option<String>,
            password: Option<String>,
            secretKey: Option<String>,
            serverSettings: BTreeMap<String, String>,
            tlsCAData: Option<String>,
            tlsSecurity: String,
            tlsServerName: Option<String>,
            user: String,
            waitUntilAvailable: String,
        }

        impl std::fmt::Display for ConfigJson {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", serde_json::to_string(self).unwrap())
            }
        }

        ConfigJson {
            address: (self.host.0.to_string(), self.host.1 as usize),
            branch: self.db.branch_for_connect().map(|s| s.to_string()),
            database: self.db.database().map(|s| s.to_string()),
            password: self.authentication.password().map(|s| s.to_string()),
            secretKey: self.authentication.secret_key().map(|s| s.to_string()),
            serverSettings: BTreeMap::from_iter(self.server_settings.clone()),
            tlsCAData: self.tls_ca.as_ref().map(|cert| to_pem(cert)),
            tlsSecurity: self.tls_security.to_string(),
            tlsServerName: self.tls_server_name.clone(),
            user: self.user.clone(),
            waitUntilAvailable: super::duration::Duration::from_micros(
                self.wait_until_available.as_micros() as i64,
            )
            .to_string(),
        }
    }

    /// Convert the config lossily to an opaque [`CredentialsFile`].
    pub fn as_credentials(&self) -> Result<CredentialsFile, CredentialsError> {
        let target = self
            .host
            .target_name()
            .map_err(|_| CredentialsError::NoTcpAddress)?;
        let tcp = target.tcp().ok_or(CredentialsError::NoTcpAddress)?;
        Ok(CredentialsFile {
            user: Some(self.user.clone()),
            host: Some(tcp.0.to_string()),
            port: Some(NonZero::new(tcp.1).expect("invalid zero port")),
            password: self.authentication.password().map(|s| s.to_string()),
            secret_key: self.authentication.secret_key().map(|s| s.to_string()),
            database: self.db.database().map(|s| s.to_string()),
            branch: self.db.branch_for_connect().map(|s| s.to_string()),
            tls_ca: self.tls_ca_pem(),
            tls_security: self.tls_security,
            tls_server_name: self.tls_server_name.clone(),
            warnings: vec![],
        })
    }

    #[allow(clippy::field_reassign_with_default)]
    pub fn to_tls(&self) -> gel_stream::TlsParameters {
        use gel_stream::{TlsAlpn, TlsCert, TlsParameters, TlsServerCertVerify};
        use std::borrow::Cow;
        use std::net::IpAddr;

        let mut tls = TlsParameters::default();
        tls.root_cert = TlsCert::Webpki;
        match &self.tls_ca {
            Some(certs) => {
                tls.root_cert = TlsCert::Custom(certs.to_vec());
            }
            None => {
                if let Some(cloud_certs) = self.cloud_certs {
                    tls.root_cert = TlsCert::WebpkiPlus(cloud_certs.certificates().to_vec());
                }
            }
        }
        tls.server_cert_verify = match self.tls_security {
            TlsSecurity::Insecure => TlsServerCertVerify::Insecure,
            TlsSecurity::NoHostVerification => TlsServerCertVerify::IgnoreHostname,
            TlsSecurity::Strict | TlsSecurity::Default => TlsServerCertVerify::VerifyFull,
        };
        tls.alpn = TlsAlpn::new_str(&["edgedb-binary", "gel-binary"]);
        tls.sni_override = match &self.tls_server_name {
            Some(server_name) => Some(Cow::from(server_name.clone())),
            None => {
                if let Ok(host) = self.host.target_name() {
                    if let Some(host) = host.host() {
                        if let Ok(ip) = IpAddr::from_str(&host) {
                            // FIXME: https://github.com/rustls/rustls/issues/184
                            let host = format!("{ip}.host-for-ip.edgedb.net");
                            // for ipv6addr
                            let host = host.replace([':', '%'], "-");
                            if host.starts_with('-') {
                                Some(Cow::from(format!("i{host}")))
                            } else {
                                Some(Cow::from(host.to_string()))
                            }
                        } else {
                            Some(Cow::from(host.to_string()))
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        tls
    }
}

/// The authentication method to use for the connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Authentication {
    #[default]
    None,
    Password(String),
    SecretKey(String),
}

impl Authentication {
    pub fn password(&self) -> Option<&str> {
        match self {
            Self::Password(password) => Some(password),
            _ => None,
        }
    }

    pub fn secret_key(&self) -> Option<&str> {
        match self {
            Self::SecretKey(secret_key) => Some(secret_key),
            _ => None,
        }
    }
}

/// The database or branch to use for the connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DatabaseBranch {
    #[default]
    Default,
    Database(String),
    Branch(String),
    Ambiguous(String),
}

impl std::fmt::Display for DatabaseBranch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Alternate display is the short form, just the name or (default)
        if f.alternate() {
            match self {
                Self::Database(database) => write!(f, "{database}"),
                Self::Branch(branch) => write!(f, "{branch}"),
                Self::Ambiguous(ambiguous) => write!(f, "{ambiguous}"),
                Self::Default => write!(f, "(default)"),
            }
        } else {
            match self {
                Self::Database(database) => write!(f, "database '{database}'"),
                Self::Branch(branch) => write!(f, "branch '{branch}'"),
                Self::Ambiguous(ambiguous) => write!(f, "'{ambiguous}'"),
                Self::Default => write!(f, "default database/branch"),
            }
        }
    }
}

impl DatabaseBranch {
    pub fn database(&self) -> Option<&str> {
        match self {
            Self::Database(database) => Some(database),
            // Special case: we return branch here
            Self::Branch(branch) => Some(branch),
            Self::Ambiguous(ambiguous) => Some(ambiguous),
            Self::Default => Some(DEFAULT_DATABASE_NAME),
        }
    }

    pub fn branch_for_connect(&self) -> Option<&str> {
        match self {
            Self::Branch(branch) => Some(branch),
            // Special case: we return database here
            Self::Database(database) => Some(database),
            Self::Ambiguous(ambiguous) => Some(ambiguous),
            Self::Default => Some(DEFAULT_BRANCH_NAME_CONNECT),
        }
    }

    pub fn branch_for_create(&self) -> Option<&str> {
        match self {
            Self::Branch(branch) => Some(branch),
            // Special case: we return database here
            Self::Database(database) => Some(database),
            Self::Ambiguous(ambiguous) => Some(ambiguous),
            Self::Default => Some(DEFAULT_BRANCH_NAME_CREATE),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Database(database) => Some(database),
            Self::Branch(branch) => Some(branch),
            Self::Ambiguous(ambiguous) => Some(ambiguous),
            Self::Default => None,
        }
    }
}

/// Client security mode.
#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub enum ClientSecurity {
    /// Disable security checks
    InsecureDevMode,
    /// Always verify domain an certificate
    Strict,
    /// Verify domain only if no specific certificate is configured
    #[default]
    Default,
}

impl FromStr for ClientSecurity {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<ClientSecurity, Self::Err> {
        use ClientSecurity::*;

        match s {
            "default" => Ok(Default),
            "strict" => Ok(Strict),
            "insecure_dev_mode" => Ok(InsecureDevMode),
            // TODO: this should have its own error?
            _ => Err(ParseError::InvalidTlsSecurity(
                TlsSecurityError::InvalidValue,
            )),
        }
    }
}

/// The type of cloud certificate to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudCerts {
    Staging,
    Local,
}

impl FromStr for CloudCerts {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<CloudCerts, Self::Err> {
        use CloudCerts::*;

        match s {
            "staging" => Ok(Staging),
            "local" => Ok(Local),
            // TODO: incorrect error
            _ => Err(ParseError::FileNotFound),
        }
    }
}

impl CloudCerts {
    pub fn certificates(&self) -> &[CertificateDer<'static>] {
        match self {
            Self::Staging => {
                static CERT: std::sync::OnceLock<Vec<CertificateDer<'static>>> =
                    std::sync::OnceLock::new();
                CERT.get_or_init(|| {
                    Self::read_static_certs(Self::Staging.certificates_pem().as_bytes())
                })
            }
            Self::Local => {
                static CERT: std::sync::OnceLock<Vec<CertificateDer<'static>>> =
                    std::sync::OnceLock::new();
                CERT.get_or_init(|| {
                    Self::read_static_certs(Self::Local.certificates_pem().as_bytes())
                })
            }
        }
    }

    pub fn certificates_pem(&self) -> &'static str {
        match self {
            Self::Staging => include_str!("certs/letsencrypt_staging.pem"),
            Self::Local => include_str!("certs/nebula_development.pem"),
        }
    }

    fn read_static_certs(bytes: &'static [u8]) -> Vec<CertificateDer<'static>> {
        let mut cursor = std::io::Cursor::new(bytes);
        let mut certs = Vec::new();
        for item in rustls_pemfile::read_all(&mut cursor) {
            match item {
                Ok(rustls_pemfile::Item::X509Certificate(data)) => certs.push(data),
                _ => panic!(),
            }
        }
        certs
    }
}

/// TLS Client Security Mode
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TlsSecurity {
    /// Allow any certificate for TLS connection
    Insecure,
    /// Verify certificate against trusted chain but allow any host name
    ///
    /// This is useful for localhost (you can't make trusted chain certificate
    /// for localhost). And when certificate of specific server is stored in
    /// credentials file so it's secure regardless of which host name was used
    /// to expose the server to the network.
    NoHostVerification,
    /// Normal TLS certificate check (checks trusted chain and hostname)
    Strict,
    /// If there is a specific certificate in credentials, do not check
    /// the host name, otherwise use `Strict` mode
    #[default]
    Default,
}

impl FromStr for TlsSecurity {
    type Err = ParseError;
    fn from_str(val: &str) -> Result<Self, Self::Err> {
        match val {
            "default" => Ok(TlsSecurity::Default),
            "insecure" => Ok(TlsSecurity::Insecure),
            "no_host_verification" => Ok(TlsSecurity::NoHostVerification),
            "strict" => Ok(TlsSecurity::Strict),
            _ => Err(ParseError::InvalidTlsSecurity(
                TlsSecurityError::InvalidValue,
            )),
        }
    }
}

impl fmt::Display for TlsSecurity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Insecure => write!(f, "insecure"),
            Self::NoHostVerification => write!(f, "no_host_verification"),
            Self::Strict => write!(f, "strict"),
            Self::Default => write!(f, "default"),
        }
    }
}

/// TCP keepalive configuration.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpKeepalive {
    /// Disable TCP keepalive probes.
    Disabled,
    /// Explicit duration between TCP keepalive probes.
    Explicit(Duration),
    /// Default: 60 seconds.
    #[default]
    Default,
}

impl TcpKeepalive {
    pub fn as_keepalive(&self) -> Option<Duration> {
        match self {
            TcpKeepalive::Disabled => None,
            TcpKeepalive::Default => Some(DEFAULT_TCP_KEEPALIVE),
            TcpKeepalive::Explicit(duration) => Some(*duration),
        }
    }
}

impl FromStr for TcpKeepalive {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use TcpKeepalive::*;

        match s {
            "disabled" => Ok(Disabled),
            "default" => Ok(Default),
            _ => Ok(Explicit(
                parse_duration(s).map_err(|_| ParseError::InvalidDuration)?,
            )),
        }
    }
}

#[derive(derive_more::Debug, Clone, PartialEq, Eq)]
enum UnixPathInner {
    /// The selected port will be appended to the path.
    #[debug("{:?}{{port}}", _0)]
    PortSuffixed(PathBuf),
    /// The path will be used as-is.
    #[debug("{:?}", _0)]
    Exact(PathBuf),
}

/// A path to a Unix socket.
#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct UnixPath {
    #[debug("{:?}", inner)]
    inner: UnixPathInner,
}

impl UnixPath {
    /// The selected port will be appended to the path.
    pub fn with_port_suffix(path: PathBuf) -> Self {
        UnixPath {
            inner: UnixPathInner::PortSuffixed(path),
        }
    }

    /// The path will be used as-is.
    pub fn exact(path: PathBuf) -> Self {
        UnixPath {
            inner: UnixPathInner::Exact(path),
        }
    }

    /// Returns a path with the port suffix appended.
    pub fn path_with_port(&self, port: u16) -> Cow<Path> {
        match &self.inner {
            UnixPathInner::PortSuffixed(path) => {
                let Some(filename) = path.file_name() else {
                    return Cow::Owned(path.join(port.to_string()));
                };
                let mut path = path.clone();
                let mut filename = filename.to_owned();
                filename.push(port.to_string());
                path.set_file_name(filename);
                Cow::Owned(path)
            }
            UnixPathInner::Exact(path) => Cow::Borrowed(path),
        }
    }
}

impl FromStr for UnixPath {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(UnixPath::exact(PathBuf::from(s)))
    }
}

impl<T: Into<PathBuf>> From<T> for UnixPath {
    fn from(path: T) -> Self {
        UnixPath::exact(path.into())
    }
}

/// Classic-style connection options.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ConnectionOptions {
    pub dsn: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub instance: Option<String>,
    pub database: Option<String>,
    pub host: Option<String>,
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub port: Option<String>,
    pub branch: Option<String>,
    #[serde(rename = "tlsSecurity")]
    pub tls_security: Option<String>,
    #[serde(rename = "tlsCA")]
    pub tls_ca: Option<String>,
    #[serde(rename = "tlsCAFile")]
    pub tls_ca_file: Option<String>,
    #[serde(rename = "tlsServerName")]
    pub tls_server_name: Option<String>,
    #[serde(rename = "waitUntilAvailable")]
    pub wait_until_available: Option<String>,
    #[serde(rename = "serverSettings")]
    pub server_settings: Option<HashMap<String, String>>,
    #[serde(rename = "credentialsFile")]
    pub credentials_file: Option<String>,
    pub credentials: Option<String>,
    #[serde(rename = "secretKey")]
    pub secret_key: Option<String>,
}

#[cfg(feature = "serde")]
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = serde_json::Value::deserialize(deserializer)?;
    if let Some(s) = s.as_str() {
        Ok(Some(s.to_string()))
    } else {
        Ok(Some(s.to_string()))
    }
}

impl TryInto<Params> for ConnectionOptions {
    type Error = ParseError;

    fn try_into(self) -> Result<Params, Self::Error> {
        if self.credentials.is_some() && self.credentials_file.is_some() {
            return Err(ParseError::MultipleCompound(
                BuildPhase::Options,
                vec![
                    CompoundSource::CredentialsFile,
                    CompoundSource::CredentialsFile,
                ],
            ));
        }

        if self.tls_ca.is_some() && self.tls_ca_file.is_some() {
            return Err(ParseError::ExclusiveOptions(
                "tls_ca".to_string(),
                "tls_ca_file".to_string(),
            ));
        }

        if self.branch.is_some() && self.database.is_some() {
            return Err(ParseError::ExclusiveOptions(
                "branch".to_string(),
                "database".to_string(),
            ));
        }

        let mut credentials = Param::from_file(self.credentials_file.clone());
        if credentials.is_none() {
            credentials = Param::from_unparsed(self.credentials.clone());
        }

        let mut tls_ca = Param::from_unparsed(self.tls_ca.clone());
        if tls_ca.is_none() {
            tls_ca = Param::from_file(self.tls_ca_file.clone());
        }

        let explicit = Params {
            dsn: Param::from_unparsed(self.dsn.clone()),
            credentials,
            user: Param::from_unparsed(self.user.clone()),
            password: Param::from_unparsed(self.password.clone()),
            instance: Param::from_unparsed(self.instance.clone()),
            database: Param::from_unparsed(self.database.clone()),
            host: Param::from_unparsed(self.host.clone()),
            port: Param::from_unparsed(self.port.as_ref().map(|n| n.to_string())),
            branch: Param::from_unparsed(self.branch.clone()),
            secret_key: Param::from_unparsed(self.secret_key.clone()),
            tls_security: Param::from_unparsed(self.tls_security.clone()),
            tls_ca,
            tls_server_name: Param::from_unparsed(self.tls_server_name.clone()),
            server_settings: self.server_settings.unwrap_or_default(),
            wait_until_available: Param::from_unparsed(self.wait_until_available.clone()),
            ..Default::default()
        };

        Ok(explicit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_credentials() {
        let config = Config::default();
        let credentials = config.as_credentials().unwrap();
        assert_eq!(credentials.host, Some("localhost".to_string()));
    }

    #[test]
    fn test_dsn_url() {
        let config = Config::default();
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel://");

        let config = Config::default().with_host("example.com", 1234).unwrap();
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel://example.com:1234");

        let config = Config::default()
            .with_host("localhost", 5656)
            .unwrap()
            .with_db(DatabaseBranch::Database("edgedb".to_string()));
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel:///edgedb");

        let config = Config::default()
            .with_host("example.com", 5656)
            .unwrap()
            .with_db(DatabaseBranch::Branch("main".to_string()));
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel://example.com/main");

        let config = Config::default()
            .with_host("localhost", 5656)
            .unwrap()
            .with_db(DatabaseBranch::Branch("main".to_string()))
            .with_user("user");
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel:///main?user=user");

        let config = Config::default()
            .with_host("localhost", 5656)
            .unwrap()
            .with_db(DatabaseBranch::Branch("main".to_string()))
            .with_user("user")
            .with_password("%[]{}");
        let url = config.dsn_url().unwrap();
        assert_eq!(url, "gel:///main?user=user&password=%25%5B%5D%7B%7D");
    }
}
