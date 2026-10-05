use std::{collections::BTreeSet, fs, path::Path};

use sorrel_core::{
    materialize_snapshot, materialize_workspace_snapshot, read_snapshot_files, InMemoryObjectStore,
    ObjectId, ObjectStore, SnapshotError, SnapshotOptions, StatCache, StatCacheEntry,
};
use tempfile::TempDir;

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn blob_id(contents: &str) -> ObjectId {
    ObjectId::for_bytes(format!("sorrel.blob.v0\n{contents}").as_bytes())
}

#[test]
fn filters_secrets_and_nested_ignore_rules_before_storing_blobs_or_using_cache() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), ".gitignore", "build/\n*.tmp\n");
    write(
        root.path(),
        ".sorrelignore",
        "private.txt\n!keep.tmp\n!.env\n",
    );
    write(root.path(), "src/.gitignore", "local.txt\n");
    write(root.path(), "src/.sorrelignore", "!local.txt\n");
    for (path, value) in [
        (".env", "SENSITIVE_DEFAULT"),
        ("src/.env.production", "SENSITIVE_NESTED"),
        (".env.example", "TOKEN=replace-me"),
        ("private.txt", "PRIVATE_IGNORED"),
        ("build/output", "BUILD_IGNORED"),
        ("src/delete.tmp", "TEMP_IGNORED"),
        ("keep.tmp", "kept by negation"),
        ("src/local.txt", "kept by Sorrel override"),
        (".git/config", "GIT_INTERNAL"),
        (".sorrel/local", "SORREL_INTERNAL"),
    ] {
        write(root.path(), path, value);
    }
    let mut cache = StatCache::new();
    // An existing cache entry must not bypass the new selection policy.
    cache.insert(
        ".env",
        StatCacheEntry {
            size: 17,
            mtime_secs: 0,
            mtime_nanos: 0,
            object_id: blob_id("SENSITIVE_DEFAULT"),
        },
    );
    cache.insert(
        "private.txt",
        StatCacheEntry {
            size: 15,
            mtime_secs: 0,
            mtime_nanos: 0,
            object_id: blob_id("PRIVATE_IGNORED"),
        },
    );
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        Some(&mut cache),
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    let paths: BTreeSet<_> = files.keys().map(|path| path.to_str().unwrap()).collect();
    assert_eq!(
        paths,
        BTreeSet::from([
            ".gitignore",
            ".sorrelignore",
            ".env.example",
            "keep.tmp",
            "src/.gitignore",
            "src/.sorrelignore",
            "src/local.txt",
        ])
    );
    assert!(cache.get(".env").is_none());
    assert!(cache.get("private.txt").is_none());
    assert!(cache.get("keep.tmp").is_some());
    for value in [
        "SENSITIVE_DEFAULT",
        "SENSITIVE_NESTED",
        "PRIVATE_IGNORED",
        "BUILD_IGNORED",
        "TEMP_IGNORED",
        "GIT_INTERNAL",
        "SORREL_INTERNAL",
    ] {
        assert!(
            !store.has(&blob_id(value)).unwrap(),
            "stored excluded content: {value}"
        );
    }
}

#[test]
fn discovers_custom_provider_paths_from_yaml_registry_and_secretspec() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(
        root.path(),
        "sorrel.secrets.yml",
        "secretRefs:\n  - id: local\n    provider: dotenv:private/yaml.env\n",
    );
    write(
        root.path(),
        ".sorrel/secrets/local.json",
        r#"{"provider":"sorrel-vault","uri":"dotenv://private/registry.env"}"#,
    );
    write(root.path(), "secretspec.toml", "[providers]\nlocal = 'dotenv:private/toml.env'\n[profiles.default.TOKEN]\nproviders = ['dotenv:private/profile.env']\n");
    for path in ["yaml", "registry", "toml", "profile"] {
        write(
            root.path(),
            &format!("private/{path}.env"),
            &format!("SENSITIVE_{path}"),
        );
    }
    write(root.path(), "private/public.txt", "public");
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    assert!(files.contains_key(Path::new("private/public.txt")));
    for path in ["yaml", "registry", "toml", "profile"] {
        assert!(!files.contains_key(Path::new(&format!("private/{path}.env"))));
        assert!(!store.has(&blob_id(&format!("SENSITIVE_{path}"))).unwrap());
    }
}

#[test]
fn keeps_regular_tracked_files_when_ignored_and_still_records_actual_deletions() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), "build/tracked.txt", "tracked");
    let baseline = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    write(root.path(), ".gitignore", "build/\n!build/untracked.txt\n");
    write(root.path(), "build/.sorrelignore", "!untracked.txt\n");
    write(root.path(), "build/untracked.txt", "IGNORED_NEW_FILE");
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        Some(&baseline.id),
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    assert!(files.contains_key(Path::new("build/tracked.txt")));
    assert!(!files.contains_key(Path::new("build/untracked.txt")));
    assert!(!store.has(&blob_id("IGNORED_NEW_FILE")).unwrap());
    fs::remove_file(root.path().join("build/tracked.txt")).unwrap();
    let deleted = materialize_workspace_snapshot(
        &store,
        root.path(),
        Some(&snapshot.id),
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    assert!(!read_snapshot_files(&store, &deleted.id)
        .unwrap()
        .contains_key(Path::new("build/tracked.txt")));
}

#[test]
fn refuses_already_tracked_secrets_before_writing_any_objects() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), ".env", "LEGACY_SECRET");
    let baseline = materialize_snapshot(&store, root.path(), SnapshotOptions::new("repo")).unwrap();
    let before = store.len();
    write(root.path(), "a-public.txt", "NEW_PUBLIC_BLOB");
    let error = materialize_workspace_snapshot(
        &store,
        root.path(),
        Some(&baseline.id),
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap_err();
    assert!(matches!(error, SnapshotError::TrackedSecret { .. }));
    assert_eq!(store.len(), before);
}

#[test]
fn malformed_provider_configuration_fails_before_writing_objects() {
    for (path, config) in [
        ("sorrel.secrets.yml", "secretRefs: ["),
        ("secretspec.toml", "providers = ["),
    ] {
        let root = TempDir::new().unwrap();
        let store = InMemoryObjectStore::new();
        write(root.path(), path, config);
        write(root.path(), "public.txt", "public");
        let error = materialize_workspace_snapshot(
            &store,
            root.path(),
            None,
            None,
            SnapshotOptions::new("repo"),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SnapshotError::WorkspaceConfiguration { .. }
        ));
        assert_eq!(store.len(), 0);
    }
}

#[test]
fn protects_absolute_encoded_and_example_provider_paths() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), "private/encoded file", "ENCODED_SECRET");
    write(root.path(), "private/absolute", "ABSOLUTE_SECRET");
    write(root.path(), ".env.example", "EXAMPLE_IS_CONFIGURED_SECRET");
    let absolute = root.path().join("private/absolute");
    write(root.path(), "sorrel.secrets.yml", &format!(
        "secretRefs:\n  - provider: 'DOTENV://private/encoded%20file?option=true#fragment'\n  - uri: 'dotenv:{}'\n  - provider: 'dotenv:.env.example'\n", absolute.display(),
    ));
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    for path in ["private/encoded file", "private/absolute", ".env.example"] {
        assert!(!files.contains_key(Path::new(path)));
    }
    for value in [
        "ENCODED_SECRET",
        "ABSOLUTE_SECRET",
        "EXAMPLE_IS_CONFIGURED_SECRET",
    ] {
        assert!(!store.has(&blob_id(value)).unwrap());
    }
}

#[test]
fn protects_vault_import_paths_and_case_variants_before_writing_blobs() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), "sorrel.secrets.yml", "localDev:\n  import:\n    envFiles:\n      - path: private/credentials\n      - path: .env.example\n");
    for (path, value) in [
        ("private/credentials", "CUSTOM_IMPORT_SECRET"),
        (".ENV.EXAMPLE", "CONFIGURED_EXAMPLE_SECRET"),
        (".ENV", "CASE_VARIANT_SECRET"),
        ("nested/.Env.Production", "NESTED_CASE_SECRET"),
        ("nested/.ENV.EXAMPLE", "PUBLIC_PLACEHOLDER"),
    ] {
        write(root.path(), path, value);
    }
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    let files = read_snapshot_files(&store, &snapshot.id).unwrap();
    assert!(files.contains_key(Path::new("nested/.ENV.EXAMPLE")));
    for (path, value) in [
        ("private/credentials", "CUSTOM_IMPORT_SECRET"),
        (".ENV.EXAMPLE", "CONFIGURED_EXAMPLE_SECRET"),
        (".ENV", "CASE_VARIANT_SECRET"),
        ("nested/.Env.Production", "NESTED_CASE_SECRET"),
    ] {
        assert!(!files.contains_key(Path::new(path)));
        assert!(!store.has(&blob_id(value)).unwrap());
    }
}

#[test]
fn malformed_vault_import_paths_fail_closed_before_writing_objects() {
    for config in [
        "localDev:\n  import:\n    envFiles: private/credentials\n",
        "localDev:\n  import:\n    envFiles:\n      - path: 42\n",
    ] {
        let root = TempDir::new().unwrap();
        let store = InMemoryObjectStore::new();
        write(root.path(), "sorrel.secrets.yml", config);
        write(root.path(), "public.txt", "public");
        let error = materialize_workspace_snapshot(
            &store,
            root.path(),
            None,
            None,
            SnapshotOptions::new("repo"),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SnapshotError::WorkspaceConfiguration { .. }
        ));
        assert_eq!(store.len(), 0);
    }
}

#[cfg(unix)]
#[test]
fn protects_the_target_of_a_configured_provider_symlink() {
    let root = TempDir::new().unwrap();
    let store = InMemoryObjectStore::new();
    write(root.path(), "actual-store", "PROVIDER_SYMLINK_SECRET");
    std::os::unix::fs::symlink("actual-store", root.path().join("provider-store")).unwrap();
    write(
        root.path(),
        "sorrel.secrets.yml",
        "secretRefs:\n  - provider: dotenv:provider-store\n",
    );
    let snapshot = materialize_workspace_snapshot(
        &store,
        root.path(),
        None,
        None,
        SnapshotOptions::new("repo"),
    )
    .unwrap();
    assert!(!read_snapshot_files(&store, &snapshot.id)
        .unwrap()
        .contains_key(Path::new("actual-store")));
    assert!(!store.has(&blob_id("PROVIDER_SYMLINK_SECRET")).unwrap());
}
