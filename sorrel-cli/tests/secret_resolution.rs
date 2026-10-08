use sorrel_cli::secretspec_bridge::{check_handles, resolve_handles, SecretHandle};
use std::fs;
use tempfile::TempDir;

fn handle(id: &str, name: &str, provider: &str, environment: &str) -> SecretHandle {
    SecretHandle {
        id: id.into(),
        name: name.into(),
        provider: provider.into(),
        uri: String::new(),
        environment: environment.into(),
        required: true,
        description: None,
    }
}

fn fixture() -> (TempDir, Vec<SecretHandle>) {
    let directory = TempDir::new().unwrap();
    fs::write(
        directory.path().join("secretspec.toml"),
        r#"
[project]
name = "sorrel-synthetic-resolution-test"
revision = "1.0"
[profiles.default]
ALPHA = { description = "Synthetic fixture", required = true }
BETA = { description = "Synthetic fixture", required = true }
[profiles.dev]
ALPHA = { description = "Synthetic fixture", required = true }
[profiles.production]
BETA = { description = "Synthetic fixture", required = true }
"#,
    )
    .unwrap();
    fs::write(directory.path().join(".first"), "ALPHA=synthetic-first\n").unwrap();
    fs::write(directory.path().join(".second"), "BETA=synthetic-second\n").unwrap();
    let handles = vec![
        handle("secret_alpha", "ALPHA", "dotenv:.first", "dev"),
        handle("secret_beta", "BETA", "dotenv:.second", "prod"),
    ];
    (directory, handles)
}

#[test]
fn resolving_and_checking_a_subset_ignore_unselected_required_secrets() {
    let (directory, handles) = fixture();
    fs::remove_file(directory.path().join(".second")).unwrap();
    let resolved =
        resolve_handles(directory.path(), &handles, &["secret_alpha".into()], None).unwrap();
    assert_eq!(resolved.values.len(), 1);
    assert_eq!(resolved.values["ALPHA"], "synthetic-first");
    assert!(!resolved.id_to_name.contains_key("secret_beta"));
    let report = check_handles(directory.path(), &handles[..1], None).unwrap();
    assert!(report.all_required_present());
    assert_eq!(report.secrets.len(), 1);
    assert_eq!(report.secrets[0].name, "ALPHA");
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("synthetic-first"));
    assert!(!directory.path().join(".second").exists());
}

#[test]
fn mixed_providers_and_profiles_resolve_independently_and_report_truthfully() {
    let (directory, handles) = fixture();
    let resolved = resolve_handles(directory.path(), &handles, &[], None).unwrap();
    assert_eq!(resolved.values["ALPHA"], "synthetic-first");
    assert_eq!(resolved.values["BETA"], "synthetic-second");
    assert_eq!(resolved.provider, "mixed");
    assert_eq!(resolved.profile, "mixed");
    let report = check_handles(directory.path(), &handles, None).unwrap();
    assert!(report.all_required_present());
    assert_eq!(report.provider, "mixed");
    assert_eq!(report.profile, "mixed");
    assert_eq!(report.secrets.len(), 2);
    assert!(report.secrets[0]
        .source_provider
        .as_deref()
        .unwrap()
        .ends_with("/.first"));
    assert!(report.secrets[1]
        .source_provider
        .as_deref()
        .unwrap()
        .ends_with("/.second"));
    let json = serde_json::to_value(report).unwrap();
    assert!(json["provider"].is_string());
    assert!(json["profile"].is_string());
    assert!(json["secrets"].is_array());
    assert!(!json.to_string().contains("synthetic-first"));
    assert!(!json.to_string().contains("synthetic-second"));
}

#[test]
fn explicit_provider_override_applies_to_every_selected_profile() {
    let (directory, handles) = fixture();
    fs::write(
        directory.path().join(".override"),
        "ALPHA=synthetic-override-first\nBETA=synthetic-override-second\n",
    )
    .unwrap();
    let resolved =
        resolve_handles(directory.path(), &handles, &[], Some(" dotenv:.override ")).unwrap();
    assert_eq!(resolved.values["ALPHA"], "synthetic-override-first");
    assert_eq!(resolved.values["BETA"], "synthetic-override-second");
    assert!(resolved.provider.ends_with("/.override"));
    assert_eq!(resolved.profile, "mixed");
    let report = check_handles(directory.path(), &handles, Some("dotenv:.override")).unwrap();
    assert!(report.all_required_present());
    assert!(report.provider.ends_with("/.override"));
    assert!(report.secrets.iter().all(|secret| secret
        .source_provider
        .as_deref()
        .unwrap()
        .ends_with("/.override")));
}

#[test]
fn missing_selected_required_secret_fails_without_exposing_values() {
    let (directory, handles) = fixture();
    fs::write(
        directory.path().join(".first"),
        "OTHER=synthetic-unselected\n",
    )
    .unwrap();
    let error = resolve_handles(directory.path(), &handles, &["secret_alpha".into()], None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ALPHA"));
    assert!(!error.contains("BETA"));
    assert!(!error.contains("synthetic-unselected"));
    let report = check_handles(directory.path(), &handles[..1], None).unwrap();
    assert!(!report.all_required_present());
    assert_eq!(
        report.secrets[0].status,
        secretspec::ResolutionStatus::MissingRequired
    );
}

#[test]
fn selected_environment_name_collisions_fail_before_resolution() {
    let (directory, mut handles) = fixture();
    handles[1].name = "ALPHA".into();
    fs::write(
        directory.path().join(".second"),
        "ALPHA=synthetic-collision\n",
    )
    .unwrap();
    for error in [
        resolve_handles(directory.path(), &handles, &[], None)
            .unwrap_err()
            .to_string(),
        check_handles(directory.path(), &handles, None)
            .unwrap_err()
            .to_string(),
    ] {
        assert!(error.contains("share environment name"));
        assert!(error.contains("secret_alpha"));
        assert!(error.contains("secret_beta"));
        assert!(!error.contains("synthetic-first"));
        assert!(!error.contains("synthetic-collision"));
    }
}

#[test]
fn narrowed_resolution_preserves_provider_aliases_and_defaults() {
    let (directory, handles) = fixture();
    fs::write(
        directory.path().join("secretspec.toml"),
        r#"
[project]
name = "sorrel-synthetic-resolution-test"
revision = "1.0"
[providers]
local = "dotenv:.first"
[profiles.default]
ALPHA = { description = "Synthetic fixture", required = true, providers = ["local"] }
BETA = { description = "Synthetic fixture", required = true }
[profiles.dev]
"#,
    )
    .unwrap();
    let resolved =
        resolve_handles(directory.path(), &handles, &["secret_alpha".into()], None).unwrap();
    assert_eq!(resolved.values["ALPHA"], "synthetic-first");
    fs::remove_file(directory.path().join(".first")).unwrap();
    fs::write(
        directory.path().join("secretspec.toml"),
        r#"
[project]
name = "sorrel-synthetic-resolution-test"
revision = "1.0"
[profiles.default]
ALPHA = { description = "Synthetic fixture", default = "synthetic-default" }
BETA = { description = "Synthetic fixture", required = true }
[profiles.dev]
"#,
    )
    .unwrap();
    let resolved =
        resolve_handles(directory.path(), &handles, &["secret_alpha".into()], None).unwrap();
    assert_eq!(resolved.values["ALPHA"], "synthetic-default");
    assert!(!directory.path().join(".first").exists());
}

#[test]
fn composition_cannot_read_unselected_dependencies() {
    let (directory, handles) = fixture();
    fs::write(
        directory.path().join("secretspec.toml"),
        r#"
[project]
name = "sorrel-synthetic-resolution-test"
revision = "1.0"
[profiles.default]
ALPHA = { description = "Synthetic fixture", composed = "${BETA}" }
BETA = { description = "Synthetic fixture", required = true }
[profiles.dev]
"#,
    )
    .unwrap();
    let error = resolve_handles(directory.path(), &handles, &["secret_alpha".into()], None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("BETA"));
    assert!(!error.contains("synthetic-second"));
}
