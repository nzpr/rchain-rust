//! Node environment initialization checks (port of `node/runtime/NodeEnvironment.scala`).
//!
//! The `create`/`name` paths (X.509 certificate parsing + comm certificate generation) are
//! deferred; only the pure data-dir / TLS file checks are ported.

use std::path::Path;

use rchain_comm::transport::tls_conf::TlsConf;

/// A node-environment initialization error (port of `NodeEnvironment.InitializationException`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializationException(pub String);

impl std::fmt::Display for InitializationException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InitializationException {}

fn data_dir_error(data_dir: &Path) -> String {
    format!(
        "The data dir must be a directory and have read and write permissions:\n{}",
        data_dir.display()
    )
}

/// Create the data dir if absent (port of `canCreateDataDir`).
pub fn can_create_data_dir(data_dir: &Path) -> Result<(), InitializationException> {
    if !data_dir.exists() {
        std::fs::create_dir(data_dir)
            .map_err(|_| InitializationException(data_dir_error(data_dir)))?;
    }
    Ok(())
}

/// Check the data dir is an accessible directory (port of `haveAccessToDataDir`).
pub fn have_access_to_data_dir(data_dir: &Path) -> Result<(), InitializationException> {
    if !data_dir.is_dir() {
        return Err(InitializationException(data_dir_error(data_dir)));
    }
    Ok(())
}

/// Check the TLS certificate file exists (port of `hasCertificate`).
pub fn has_certificate(tls: &TlsConf) -> Result<(), InitializationException> {
    if !tls.certificate_path.exists() {
        return Err(InitializationException(format!(
            "Certificate file {} not found",
            tls.certificate_path.display()
        )));
    }
    Ok(())
}

/// Check the TLS secret-key file exists (port of `hasKey`).
pub fn has_key(tls: &TlsConf) -> Result<(), InitializationException> {
    if !tls.key_path.exists() {
        return Err(InitializationException(format!(
            "Secret key file {} not found",
            tls.key_path.display()
        )));
    }
    Ok(())
}
