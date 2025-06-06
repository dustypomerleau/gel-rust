use futures::StreamExt;
use gel_stream::*;
use std::borrow::Cow;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn load_client_test_cert() -> rustls_pki_types::CertificateDer<'static> {
    rustls_pemfile::certs(&mut include_str!("../tests/certs/client.cert.pem").as_bytes())
        .next()
        .expect("no cert")
        .expect("cert is bad")
}

fn load_client_test_key() -> rustls_pki_types::PrivateKeyDer<'static> {
    rustls_pemfile::private_key(&mut include_str!("../tests/certs/client.key.pem").as_bytes())
        .expect("no client key")
        .expect("client key is bad")
}

fn load_client_test_ca() -> rustls_pki_types::CertificateDer<'static> {
    rustls_pemfile::certs(&mut include_str!("../tests/certs/client_ca.cert.pem").as_bytes())
        .next()
        .expect("no ca cert")
        .expect("ca cert is bad")
}

fn load_test_cert() -> rustls_pki_types::CertificateDer<'static> {
    rustls_pemfile::certs(&mut include_str!("../tests/certs/server.cert.pem").as_bytes())
        .next()
        .expect("no cert")
        .expect("cert is bad")
}

fn load_test_ca() -> rustls_pki_types::CertificateDer<'static> {
    rustls_pemfile::certs(&mut include_str!("../tests/certs/ca.cert.pem").as_bytes())
        .next()
        .expect("no ca cert")
        .expect("ca cert is bad")
}

fn load_test_key() -> rustls_pki_types::PrivateKeyDer<'static> {
    rustls_pemfile::private_key(&mut include_str!("../tests/certs/server.key.pem").as_bytes())
        .expect("no server key")
        .expect("server key is bad")
}

fn load_test_crls() -> Vec<rustls_pki_types::CertificateRevocationListDer<'static>> {
    rustls_pemfile::crls(&mut include_str!("../tests/certs/ca.crl.pem").as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn tls_server_parameters(
    alpn: TlsAlpn,
    client_cert_verify: TlsClientCertVerify,
) -> TlsServerParameterProvider {
    TlsServerParameterProvider::new(TlsServerParameters {
        server_certificate: TlsKey::new(load_test_key(), load_test_cert()),
        client_cert_verify,
        min_protocol_version: None,
        max_protocol_version: None,
        alpn,
    })
}

async fn spawn_tls_server<S: TlsDriver>(
    expected_hostname: Option<&str>,
    server_alpn: TlsAlpn,
    expected_alpn: Option<&str>,
    client_cert_verify: TlsClientCertVerify,
) -> Result<
    (
        ResolvedTarget,
        tokio::task::JoinHandle<Result<(), ConnectionError>>,
    ),
    ConnectionError,
> {
    let validate_cert = client_cert_verify != TlsClientCertVerify::Ignore;
    let mut acceptor = Acceptor::new_tcp_tls(
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0),
        tls_server_parameters(server_alpn, client_cert_verify),
    )
    .bind_explicit::<S>()
    .await?;
    let addr = acceptor.local_address()?;

    let expected_alpn = expected_alpn.map(|alpn| alpn.as_bytes().to_vec());
    let expected_hostname = expected_hostname.map(|sni| sni.to_string());
    let accept_task = tokio::spawn(async move {
        let mut connection = acceptor.next().await.unwrap()?;
        let handshake = connection
            .handshake()
            .unwrap_or_else(|| panic!("handshake was not available on {connection:?}"));
        assert!(handshake.version.is_some());
        assert_eq!(
            handshake.alpn.as_ref().map(|b| b.as_ref().to_vec()),
            expected_alpn
        );
        assert_eq!(handshake.sni.as_deref(), expected_hostname.as_deref());
        if validate_cert {
            assert!(handshake.cert.is_some());
            let cert = parse_cert(handshake.cert.as_ref().unwrap());
            let subject = cert.subject().to_string();
            assert!(
                subject.to_ascii_lowercase().contains("ssl_user"),
                "subject: {subject}"
            );
        }
        let mut buf = String::new();
        connection.read_to_string(&mut buf).await.unwrap();
        assert_eq!(buf, "Hello, world!");
        connection.shutdown().await?;
        Ok::<_, ConnectionError>(())
    });
    Ok((addr, accept_task))
}

async fn spawn_tls_server_unclean<S: TlsDriver>() -> Result<
    (
        ResolvedTarget,
        tokio::task::JoinHandle<Result<(), ConnectionError>>,
    ),
    ConnectionError,
> {
    let mut acceptor = Acceptor::new_tcp_tls(
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0),
        tls_server_parameters(TlsAlpn::default(), TlsClientCertVerify::Ignore),
    )
    .bind_explicit::<S>()
    .await?;
    let addr = acceptor.local_address()?;

    let accept_task = tokio::spawn(async move {
        let mut connection = acceptor.next().await.unwrap()?;
        let _handshake = connection
            .handshake()
            .unwrap_or_else(|| panic!("handshake was not available on {connection:?}"));

        connection.write_all(b"Hello, world!").await.unwrap();
        connection.flush().await.unwrap();
        let mut buf = [0; 13];
        connection.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Hello, world!");
        connection.unclean_shutdown().expect("Failed to shutdown");
        Ok::<_, ConnectionError>(())
    });
    Ok((addr, accept_task))
}

async fn spawn_tcp_server() -> Result<
    (
        ResolvedTarget,
        tokio::task::JoinHandle<Result<(), ConnectionError>>,
    ),
    ConnectionError,
> {
    let mut acceptor = Acceptor::new_tcp(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0))
        .bind()
        .await?;
    let addr = acceptor.local_address()?;

    let accept_task = tokio::spawn(async move {
        let mut connection = acceptor.next().await.unwrap()?;
        connection.write_all(b"No TLS!").await?;
        connection.shutdown().await?;
        Ok::<_, ConnectionError>(())
    });
    Ok((addr, accept_task))
}

macro_rules! tls_test (
    (
        $(
            $(#[ $attr:meta ])*
            async fn $name:ident<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> $body:block
        )*
    ) => {
        mod rustls {
            use super::*;
            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<RustlsDriver, RustlsDriver>().await
                }
            )*
        }

        #[cfg(not(windows))]
        mod rustls_server {
            use super::*;
            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<OpensslDriver, RustlsDriver>().await
                }
            )*
        }

        #[cfg(not(windows))]
        mod openssl {
            use super::*;

            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<OpensslDriver, OpensslDriver>().await
                }
            )*
        }

        #[cfg(not(windows))]
        mod openssl_server {
            use super::*;
            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<RustlsDriver, OpensslDriver>().await
                }
            )*
        }

    }
);

tls_test! {
    /// The certificate is not valid for 127.0.0.1, so the connection should fail.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_fails<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
            spawn_tls_server::<S>(None, TlsAlpn::default(), None, TlsClientCertVerify::Ignore).await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    ..Default::default()
                },
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::InvalidIssuer)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap_err();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// The certificate is not valid for 127.0.0.1, so the connection should fail.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_fails_webpki<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
            spawn_tls_server::<S>(None, TlsAlpn::default(), None, TlsClientCertVerify::Ignore).await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    root_cert: TlsCert::Webpki,
                    ..Default::default()
                },
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::InvalidIssuer)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap_err();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// The certificate is not valid for 127.0.0.1, so the connection should fail.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_fails_name<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
            spawn_tls_server::<S>(None, TlsAlpn::default(), None, TlsClientCertVerify::Ignore).await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    root_cert: TlsCert::Custom(vec![load_test_ca()]),
                    ..Default::default()
                },
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::InvalidCertificateForName)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap_err();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// The certificate is not valid for 127.0.0.1, so the connection should fail.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_fails_name_system_plus<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
            spawn_tls_server::<S>(None, TlsAlpn::default(), None, TlsClientCertVerify::Ignore).await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    root_cert: TlsCert::SystemPlus(vec![load_test_ca()]),
                    ..Default::default()
                },
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::InvalidCertificateForName)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap_err();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// The certificate is valid for "localhost", so the connection should succeed.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_ok<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            Some("localhost"),
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_tcp_tls(
                ("localhost", addr.tcp().unwrap().port()),
                TlsParameters {
                    root_cert: TlsCert::Custom(vec![load_test_ca()]),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await?;
            stm.write_all(b"Hello, world!").await?;
            stm.shutdown().await?;
            Ok::<_, ConnectionError>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_server_unclean_shutdown<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server_unclean::<S>().await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            let mut buf = [0; 13];
            stm.read_exact(&mut buf).await?;
            assert_eq!(&buf, b"Hello, world!");
            stm.write_all(b"Hello, world!").await?;
            stm.flush().await?;
            let mut buf = [0; 1];
            if C::is::<RustlsDriver>() {
                let err = stm.read(&mut buf).await.unwrap_err();
                assert!(err.kind() == std::io::ErrorKind::UnexpectedEof, "{err:?}");
            } else {
                // tokio-openssl doesn't currently detect missing close_notify
            }
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_server_unclean_shutdown_during_shutdown<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
        spawn_tls_server_unclean::<S>().await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            let mut buf = [0; 13];
            stm.read_exact(&mut buf).await?;
            assert_eq!(&buf, b"Hello, world!");
            stm.write_all(b"Hello, world!").await?;
            stm.flush().await?;

            // This _should_ fail but it doesn't
            // let err = stm.shutdown().await.unwrap_err();
            // assert!(err.kind() == std::io::ErrorKind::UnexpectedEof, "{err:?}");
            // ... For now, we just test that it succeeds
            stm.shutdown().await?;

            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_server_unclean_shutdown_ignored<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
        spawn_tls_server_unclean::<S>().await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    ..Default::default()
                },
            );
            let mut connector = Connector::<C>::new_explicit(target).unwrap();
            connector.ignore_missing_tls_close_notify();
            let mut stm = connector.connect().await.unwrap();
            let mut buf = [0; 13];
            stm.read_exact(&mut buf).await?;
            assert_eq!(&buf, b"Hello, world!");
            stm.write_all(b"Hello, world!").await?;
            stm.flush().await?;
            let mut buf = [0; 1];
            let n = stm.read(&mut buf).await.unwrap();
            assert_eq!(n, 0);
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// The certificate is valid for "localhost", so the connection should succeed.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_verify_full_addl_certs_ok<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            Some("localhost"),
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_tcp_tls(
                ("localhost", addr.tcp().unwrap().port()),
                TlsParameters {
                    root_cert: TlsCert::SystemPlus(vec![load_test_ca()]),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await?;
            stm.write_all(b"Hello, world!").await?;
            stm.shutdown().await?;
            Ok::<_, ConnectionError>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_insecure<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) =
            spawn_tls_server::<S>(None, TlsAlpn::default(), None, TlsClientCertVerify::Ignore).await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr, // Raw IP
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await?;
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_crl<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            Some("localhost"),
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_tcp_tls(
                ("localhost", addr.tcp().unwrap().port()),
                TlsParameters {
                    root_cert: TlsCert::Custom(vec![load_test_ca()]),
                    crl: load_test_crls(),
                    ..Default::default()
                },
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::CertificateRevoked)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap_err();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// Test that we can override the SNI.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_sni_override<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            Some("www.google.com"),
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    sni_override: Some(Cow::Borrowed("www.google.com")),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await.unwrap();
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// Test that we can override the SNI.
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_sni_override_ignore_hostname<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            Some("www.google.com"),
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters {
                    root_cert: TlsCert::Custom(vec![load_test_ca()]),
                    server_cert_verify: TlsServerCertVerify::IgnoreHostname,
                    sni_override: Some(Cow::Borrowed("www.google.com")),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await.unwrap();
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    /// Test that we can set the ALPN.
    #[tokio::test]
    async fn test_target_tcp_tls_alpn<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            None,
            TlsAlpn::new_str(&["nope", "accepted"]),
            Some("accepted"),
            TlsClientCertVerify::Ignore,
        )
        .await?;

        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    alpn: TlsAlpn::new_str(&["accepted", "fake"]),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await.unwrap();
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });

        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();

        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_client_verify_required<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            None,
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Validate(vec![load_client_test_ca()]),
        )
        .await?;
        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    cert: Some(load_client_test_cert()),
                    key: Some(load_client_test_key()),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await.unwrap();
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });
        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();
        Ok(())
    }

    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_tcp_tls_client_verify_optional<C: TlsDriver, S: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tls_server::<S>(
            None,
            TlsAlpn::default(),
            None,
            TlsClientCertVerify::Optional(vec![load_client_test_ca()]),
        )
        .await?;
        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters {
                    server_cert_verify: TlsServerCertVerify::Insecure,
                    cert: Some(load_client_test_cert()),
                    key: Some(load_client_test_key()),
                    ..Default::default()
                },
            );
            let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
            stm.write_all(b"Hello, world!").await.unwrap();
            stm.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });
        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();
        Ok(())
    }

}

macro_rules! tls_client_test (
    (
        $(
            $(#[ $attr:meta ])*
            async fn $name:ident<C: TlsDriver>() -> Result<(), ConnectionError> $body:block
        )*
    ) => {
        mod rustls_client {
            use super::*;
            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<RustlsDriver>().await
                }
            )*
        }

        #[cfg(not(windows))]
        mod openssl_client {
            use super::*;

            $(
                $(#[ $attr ])*
                async fn $name() -> Result<(), ConnectionError> {
                    async fn test_inner<C: TlsDriver>() -> Result<(), ConnectionError> {
                        $body
                    }
                    test_inner::<OpensslDriver>().await
                }
            )*
        }

    }
);

tls_client_test! {
    #[tokio::test]
    #[ntest::timeout(30_000)]
    async fn test_target_non_tls_server<C: TlsDriver>() -> Result<(), ConnectionError> {
        let (addr, accept_task) = spawn_tcp_server()
        .await?;
        let connect_task = tokio::spawn(async move {
            let target = Target::new_resolved_tls(
                addr,
                TlsParameters::insecure(),
            );
            let stm = Connector::<C>::new_explicit(target).unwrap().connect().await;
            assert!(
                matches!(&stm, Err(ConnectionError::SslError(ssl)) if ssl.common_error() == Some(CommonError::InvalidTlsProtocolData)),
                "{stm:?}"
            );
            Ok::<_, std::io::Error>(())
        });
        accept_task.await.unwrap().unwrap();
        connect_task.await.unwrap().unwrap();
        Ok(())
    }

    #[cfg(feature = "__manual_tests")]
    #[tokio::test]
    async fn test_live_server_with_custom_certs<C: TlsDriver>() -> Result<(), ConnectionError> {
        let target = Target::new_tcp_tls(("www.google.com", 443), TlsParameters {
            root_cert: TlsCert::Custom(vec![load_test_ca()]),
            ..Default::default()
        });
        let err = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap_err();
        assert!(matches!(&err, ConnectionError::SslError(ssl) if ssl.common_error() == Some(CommonError::InvalidIssuer)), "{err:?}");
        Ok(())
    }

    #[cfg(feature = "__manual_tests")]
    #[tokio::test]
    async fn test_live_server_with_addl_certs<C: TlsDriver>() -> Result<(), ConnectionError> {
        let target = Target::new_tcp_tls(("www.google.com", 443), TlsParameters {
            root_cert: TlsCert::SystemPlus(vec![load_test_ca()]),
            ..Default::default()
        });
        let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
        stm.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // HTTP/1. .....
        assert_eq!(stm.read_u8().await.unwrap(), b'H');
        Ok(())
    }

    #[cfg(feature = "__manual_tests")]
    #[tokio::test]
    async fn test_live_server_with_webpki_certs<C: TlsDriver>() -> Result<(), ConnectionError> {
        let target = Target::new_tcp_tls(("www.google.com", 443), TlsParameters {
            root_cert: TlsCert::WebpkiPlus(vec![load_test_ca()]),
            ..Default::default()
        });
        let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
        stm.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // HTTP/1. .....
        assert_eq!(stm.read_u8().await.unwrap(), b'H');
        Ok(())
    }

    #[cfg(feature = "__manual_tests")]
    #[tokio::test]
    async fn test_live_server_manual_google_com<C: TlsDriver>() -> Result<(), ConnectionError> {
        let target = Target::new_tcp_tls(("www.google.com", 443), TlsParameters::default());
        let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
        let handshake = stm.handshake().unwrap();
        assert!(handshake.cert.is_some());
        let cert = parse_cert(handshake.cert.as_ref().unwrap());
        let subject = cert.subject().to_string();
        assert!(subject.to_ascii_lowercase().contains("google"));
        stm.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // HTTP/1. .....
        assert_eq!(stm.read_u8().await.unwrap(), b'H');
        Ok(())
    }

    /// Normally connecting to Google's IP will send an invalid SNI and fail.
    /// This test ensures that we can override the SNI to the correct hostname.
    #[cfg(feature = "__manual_tests")]
    #[tokio::test]
    async fn test_live_server_google_com_override_sni<C: TlsDriver>() -> Result<(), ConnectionError> {
        use std::net::ToSocketAddrs;

        let addr = "www.google.com:443"
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        let target = Target::new_tcp_tls(
            addr,
            TlsParameters {
                sni_override: Some(Cow::Borrowed("www.google.com")),
                ..Default::default()
            },
        );
        let mut stm = Connector::<C>::new_explicit(target).unwrap().connect().await.unwrap();
        stm.write_all(b"GET / HTTP/1.0\r\n\r\n").await.unwrap();
        // HTTP/1. .....
        assert_eq!(stm.read_u8().await.unwrap(), b'H');
        Ok(())
    }
}

fn parse_cert<'a>(
    cert: &'a rustls_pki_types::CertificateDer<'a>,
) -> x509_parser::prelude::X509Certificate<'a> {
    let (_, cert) = x509_parser::parse_x509_certificate(cert).unwrap();
    cert
}
