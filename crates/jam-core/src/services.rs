//! Build-time service configuration. Normal releases have no runtime override.
//!
//! Fixture artifacts exercise the same HTTP, TLS, signature and installer code
//! against isolated services. They trust their own certificate and signing key;
//! they never turn verification off.

#[cfg(not(feature = "fixture-services"))]
pub const RELEASE_ORIGIN: &str = "https://github.com/saffronjam/jam/releases/download";
#[cfg(not(feature = "fixture-services"))]
pub const RELEASE_LATEST: &str = "https://api.github.com/repos/saffronjam/jam/releases/latest";
#[cfg(not(feature = "fixture-services"))]
pub const UPDATE_KEY: &str = include_str!("../../../assets/update-public-key.hex");
#[cfg(not(feature = "fixture-services"))]
pub const CODEX_USAGE: &str = "https://chatgpt.com/backend-api/codex/usage";
#[cfg(not(feature = "fixture-services"))]
pub const CLAUDE_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";
#[cfg(not(feature = "fixture-services"))]
pub const CLAUDE_PROFILE: &str = "https://api.anthropic.com/api/oauth/profile";
#[cfg(not(feature = "fixture-services"))]
pub const CLAUDE_API: &str = "https://api.anthropic.com";

#[cfg(feature = "fixture-services")]
pub const RELEASE_ORIGIN: &str = concat!(env!("JAM_FIXTURE_ORIGIN"), "/releases/download");
#[cfg(feature = "fixture-services")]
pub const RELEASE_LATEST: &str = concat!(env!("JAM_FIXTURE_ORIGIN"), "/releases/latest");
#[cfg(feature = "fixture-services")]
pub const UPDATE_KEY: &str = include_str!(env!("JAM_FIXTURE_UPDATE_KEY_FILE"));
#[cfg(feature = "fixture-services")]
pub const CODEX_USAGE: &str = concat!(env!("JAM_FIXTURE_ORIGIN"), "/codex/usage");
#[cfg(feature = "fixture-services")]
pub const CLAUDE_USAGE: &str = concat!(env!("JAM_FIXTURE_ORIGIN"), "/claude/usage");
#[cfg(feature = "fixture-services")]
pub const CLAUDE_PROFILE: &str = concat!(env!("JAM_FIXTURE_ORIGIN"), "/claude/profile");

#[cfg(feature = "fixture-services")]
pub const CLAUDE_API: &str = env!("JAM_FIXTURE_ORIGIN");

pub fn tls_config() -> ureq::tls::TlsConfig {
    #[cfg(feature = "fixture-services")]
    {
        use ureq::tls::{Certificate, RootCerts, TlsConfig};
        let certificate = Certificate::from_pem(include_bytes!(env!("JAM_FIXTURE_CA_FILE")))
            .expect("fixture CA validated by artifact build");
        TlsConfig::builder()
            .root_certs(RootCerts::Specific(vec![certificate].into()))
            .build()
    }
    #[cfg(not(feature = "fixture-services"))]
    ureq::tls::TlsConfig::default()
}
