//! Release ordering at connection and installation boundaries.
use anyhow::{Context, Result};

#[derive(Debug)]
pub struct UpgradeRequired {
    pub current: String,
    pub minimum: String,
}
impl std::fmt::Display for UpgradeRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This machine runs Jam {}. Upgrade this client (Jam {}) before connecting; the server was left unchanged",
            self.minimum, self.current
        )
    }
}
impl std::error::Error for UpgradeRequired {}

/// Never infer ordering from protocol numbers or source fingerprints.
pub fn require_client_at_least(server: Option<&str>, client: &str) -> Result<()> {
    let server =
        server.context("Server has no release version; explicit server migration is required")?;
    let server = semver::Version::parse(server).context("Invalid server release version")?;
    let client = semver::Version::parse(client).context("Invalid client release version")?;
    if server > client {
        return Err(UpgradeRequired {
            current: client.to_string(),
            minimum: server.to_string(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newer_remote_requires_client_upgrade() {
        assert!(require_client_at_least(Some("0.0.2"), "0.0.1").is_err());
        assert!(require_client_at_least(Some("0.0.10"), "0.0.2").is_err());
        assert!(require_client_at_least(Some("0.0.2"), "0.0.2").is_ok());
        assert!(require_client_at_least(Some("0.0.1"), "0.0.2").is_ok());
    }
    #[test]
    fn unknown_or_malformed_never_means_older() {
        assert!(require_client_at_least(None, "0.0.1").is_err());
        assert!(require_client_at_least(Some("jam/6"), "0.0.1").is_err());
    }
}
