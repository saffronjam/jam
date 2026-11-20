fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--build-info") {
        println!(
            "{}",
            serde_json::json!({"release_version":jam_core::RELEASE_VERSION,"build":jam_core::SERVER_BUILD,"protocol":jam_core::VERSION,"arch":std::env::consts::ARCH})
        );
        return Ok(());
    }
    jam_core::server::serve()
}
