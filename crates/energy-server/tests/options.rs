//! The options document: defaults, validation and the redacted summary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use energy_core::safety::Authority;
use energy_server::bootstrap::options::Options;

#[test]
fn missing_options_are_env_only_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let options = Options::at(dir.path()).unwrap();
    assert_eq!(options.mode(), Authority::DryRun);
    assert_eq!(options.manual_batteries().len(), 0);
    assert!(options.mdns());
}

#[test]
fn batteries_require_an_address_and_a_serial() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("options.json");
    std::fs::write(
        &path,
        r#"{"batteries":"[{\"address\":\"10.0.0.2\",\"serial\":\"ABC\",\"port\":8080}]"}"#,
    )
    .unwrap();
    let options = Options::at(dir.path()).unwrap();
    assert_eq!(
        options.manual_batteries(),
        vec![("10.0.0.2".to_owned(), "ABC".to_owned(), 8080)]
    );

    std::fs::write(&path, r#"{"batteries":"[{\"address\":\"10.0.0.2\"}]"}"#).unwrap();
    assert!(Options::at(dir.path()).is_err());
    std::fs::write(&path, r#"{"batteries":"not json"}"#).unwrap();
    assert!(Options::at(dir.path()).is_err());
}

#[test]
fn the_debug_output_redacts_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let options = Options::at(dir.path())
        .unwrap()
        .with_direct_ha("http://127.0.0.1:8123", "supersecret");
    let text = format!("{options:?}");
    assert!(!text.contains("supersecret"));
}
