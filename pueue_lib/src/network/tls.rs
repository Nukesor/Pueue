use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::{TlsAcceptor, TlsConnector};

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_pemfile::{pkcs8_private_keys, rsa_private_keys};

use crate::error::Error;
use crate::settings::Shared;

/// Initialize our client [TlsConnector]. \
/// 1. Trust our own CA. ONLY our own CA.
/// 2. Set the client certificate and key
pub async fn get_tls_connector(settings: &Shared) -> Result<TlsConnector, Error> {
    // Only trust server-certificates signed with our own CA.
    let ca = load_ca(&settings.daemon_cert())?;
    let mut cert_store = RootCertStore::empty();
    cert_store.add(ca).map_err(|err| {
        Error::CertificateFailure(format!("Failed to build RootCertStore: {err}"))
    })?;

    let config: ClientConfig = ClientConfig::builder()
        .with_root_certificates(cert_store)
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

/// Configure the server using rusttls. \
/// A TLS server needs a certificate and a fitting private key.
pub fn get_tls_listener(settings: &Shared) -> Result<TlsAcceptor, Error> {
    // Set the server-side key and certificate that should be used for all communication.
    let certs = load_certs(&settings.daemon_cert())?;
    let key = load_key(&settings.daemon_key())?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| Error::CertificateFailure(format!("Failed to build TLS Acceptor: {err}")))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Load the passed certificates file
fn load_certs<'a>(path: &Path) -> Result<Vec<CertificateDer<'a>>, Error> {
    let file = File::open(path)
        .map_err(|err| Error::IoPathError(path.to_path_buf(), "opening cert", err))?;
    let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|_| Error::CertificateFailure("Failed to parse daemon certificate.".into()))?
        .into_iter()
        .collect();

    Ok(certs)
}

/// Load the passed keys file.
/// Only the first key will be used. It should match the certificate.
fn load_key<'a>(path: &Path) -> Result<PrivateKeyDer<'a>, Error> {
    let file = File::open(path)
        .map_err(|err| Error::IoPathError(path.to_path_buf(), "opening key", err))?;

    // Try to read pkcs8 format first
    let keys = pkcs8_private_keys(&mut BufReader::new(&file))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|_| Error::CertificateFailure("Failed to parse pkcs8 format.".into()));

    if let Ok(keys) = keys {
        if let Some(key) = keys.into_iter().next() {
            return Ok(PrivateKeyDer::Pkcs8(key));
        }
    }

    // Try the normal rsa format afterwards.
    let keys = rsa_private_keys(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|_| Error::CertificateFailure("Failed to parse daemon key.".into()))?;

    if let Some(key) = keys.into_iter().next() {
        return Ok(PrivateKeyDer::Pkcs1(key));
    }

    Err(Error::CertificateFailure(format!(
        "Couldn't extract private key from keyfile {path:?}",
    )))
}

fn load_ca<'a>(path: &Path) -> Result<CertificateDer<'a>, Error> {
    let file = File::open(path)
        .map_err(|err| Error::IoPathError(path.to_path_buf(), "opening cert", err))?;

    let cert = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|_| Error::CertificateFailure("Failed to parse daemon certificate.".into()))?
        .into_iter()
        .next()
        .ok_or_else(|| Error::CertificateFailure("Couldn't find CA certificate in file".into()))?;

    Ok(cert)
}
