use crate::prelude::*;

use std::fs::File;
use std::io::Write;
use std::path::Path;

use rcgen::{generate_simple_self_signed, CertifiedKey};

use crate::error::Error;
use crate::settings::Shared;

/// This the default certificates at the default `pueue_dir/certs` location.
pub fn create_certificates(shared_settings: &Shared) -> Result<(), Error> {
    let daemon_cert_path = shared_settings.daemon_cert();
    let daemon_key_path = shared_settings.daemon_key();

    if daemon_key_path.exists() || daemon_cert_path.exists() {
        if !(daemon_key_path.exists() && daemon_cert_path.exists()) {
            return Err(Error::CertificateFailure(
                "Not all default certificates exist, some are missing. \
                 Please fix your cert/key paths.\n \
                 You can also remove the `$pueue_directory/certs` directory \
                 and restart the daemon to create new certificates/keys."
                    .into(),
            ));
        }
        info!("All default keys do exist.");
        return Ok(());
    }

    let subject_alt_names = vec!["pueue.local".to_string(), "localhost".to_string()];

    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(subject_alt_names).map_err(|_| {
            Error::CertificateFailure("Failed to generate self-signed daemon certificate.".into())
        })?;
    // The certificate is now valid for localhost and the domain "hello.world.example"
    let ca_cert = cert.pem();
    write_file(ca_cert, "daemon cert", &daemon_cert_path)?;

    let ca_key = key_pair.serialize_pem();
    write_file(ca_key, "daemon key", &daemon_key_path)?;

    Ok(())
}

fn write_file(blob: String, name: &str, path: &Path) -> Result<(), Error> {
    info!("Generate {name}.");
    let mut file = File::create(path)
        .map_err(|err| Error::IoPathError(path.to_path_buf(), "creating certificate", err))?;

    file.write_all(&blob.into_bytes())
        .map_err(|err| Error::IoPathError(path.to_path_buf(), "writing certificate", err))?;

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = file
            .metadata()
            .map_err(|_| Error::CertificateFailure("Failed to certificate permission.".into()))?
            .permissions();
        permissions.set_mode(0o640);
        std::fs::set_permissions(path, permissions)
            .map_err(|_| Error::CertificateFailure("Failed to certificate permission.".into()))?;
    }

    Ok(())
}
