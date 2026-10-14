//! File selection for local workspaces, shared by the CLI and Rust SDK.
//! Low-level directory materialization remains available for importing arbitrary trees.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Serialize;
use serde_json::Value;

use crate::{
    read_snapshot, read_tree, snapshot::write_tree_from_dir, write_snapshot, write_tree, EntryType,
    ObjectId, ObjectKind, ObjectRef, ObjectStore, Snapshot, SnapshotError, SnapshotOptions,
    SnapshotResult, StatCache, Tree, TreeEntry,
};

/// Snapshots a workspace with its nested `.gitignore` and `.sorrelignore` rules.
///
/// Regular files in `baseline` remain tracked even when newly ignored. Secret-provider
/// paths are always protected; a tracked secret causes an error before any objects
/// are written. `.env.example` is a regular file unless configured as a provider.
pub fn materialize_workspace_snapshot(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    baseline: Option<&ObjectId>,
    stat_cache: Option<&mut StatCache>,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot> {
    let root = root.as_ref();
    let mut selection = WorkspaceSelection::load(store, root, baseline)?;
    selection.validate_tracked_paths()?;
    let excluded = [".sorrel", ".git"].map(std::ffi::OsString::from).into();
    let mut paths_seen = BTreeSet::new();
    let mut stat_cache = stat_cache;
    let tree = write_tree_from_dir(
        store,
        root,
        Path::new(""),
        &excluded,
        stat_cache.as_deref_mut(),
        Some(&mut paths_seen),
        Some(&mut selection),
    )?;
    if let Some(cache) = stat_cache {
        cache.retain(&paths_seen);
    }
    write_snapshot(store, tree.id, options)
}

/// Records literal files or directory subtrees over `baseline`, preserving every
/// unselected entry. Empty `paths` retains full-workspace semantics. Selectors
/// are normalized and validated before writing objects; ignored, protected,
/// metadata, unknown, and symlink paths are rejected. Select the replaced path
/// itself for a file/directory transition, rather than only a descendant.
/// Working files are never changed. Cache entries describe observed file bytes,
/// not the resulting snapshot; unvisited entries are dropped conservatively.
pub fn materialize_workspace_snapshot_paths(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    baseline: &ObjectId,
    paths: &[PathBuf],
    stat_cache: Option<&mut StatCache>,
    options: SnapshotOptions,
) -> SnapshotResult<Snapshot> {
    if paths.is_empty() {
        return materialize_workspace_snapshot(store, root, Some(baseline), stat_cache, options);
    }
    let root = root.as_ref();
    let mut selection = WorkspaceSelection::load(store, root, Some(baseline))?;
    selection.validate_tracked_paths()?;
    let mut selectors = BTreeSet::new();
    for path in paths {
        let explanation = explain_path(&mut selection, path)?;
        let relative = normalize_selector(path)?;
        let tracked = selection
            .tracked
            .iter()
            .any(|path| path.starts_with(&relative))
            || selection.tracked_directories.contains(&relative);
        if path.as_os_str().is_empty()
            || !explanation.included
            || explanation.ignored
            || (explanation.exists != Some(true) && !tracked)
        {
            return Err(config_error(path, "selector must name a visible file/directory or tracked deletion; ignored, protected, metadata and unsupported paths cannot be selected"));
        }
        selectors.insert(relative);
    }
    // Selecting a descendant must not silently replace an unselected tracked file.
    for path in &selectors {
        for ancestor in path.ancestors().skip(1) {
            if selection.tracked.contains(ancestor)
                && !selectors
                    .iter()
                    .any(|selected| ancestor.starts_with(selected))
            {
                return Err(config_error(
                    path,
                    "file/directory replacement requires selecting the replaced path",
                ));
            }
        }
    }
    selection.selectors = Some(selectors);
    let excluded = [".sorrel", ".git"].map(std::ffi::OsString::from).into();
    let mut paths_seen = BTreeSet::new();
    let mut stat_cache = stat_cache;
    let current = write_tree_from_dir(
        store,
        root,
        Path::new(""),
        &excluded,
        stat_cache.as_deref_mut(),
        Some(&mut paths_seen),
        Some(&mut selection),
    )?;
    let base = read_snapshot(store, baseline)?;
    let tree = overlay_selected_tree(
        store,
        read_tree(store, &base.root_tree.id)?.entries,
        current.entries,
        selection.selectors.as_ref().unwrap(),
    )?;
    let snapshot = write_snapshot(store, tree.id, options)?;
    if let Some(cache) = stat_cache {
        cache.retain(&paths_seen);
    }
    Ok(snapshot)
}

fn overlay_selected_tree(
    store: &impl ObjectStore,
    base: Vec<TreeEntry>,
    current: Vec<TreeEntry>,
    selectors: &BTreeSet<PathBuf>,
) -> SnapshotResult<Tree> {
    let mut current: BTreeMap<_, _> = current.into_iter().map(|e| (e.path.clone(), e)).collect();
    let mut entries = Vec::new();
    for mut entry in base {
        let observed = current.remove(&entry.path);
        if selectors.iter().any(|path| entry.path.starts_with(path)) {
            entries.extend(observed);
        } else if entry.entry_type == EntryType::Directory
            && selectors.iter().any(|path| path.starts_with(&entry.path))
        {
            let children = observed
                .map(|entry| read_tree(store, &entry.object.id))
                .transpose()?
                .map(|tree| tree.entries)
                .unwrap_or_default();
            let tree = overlay_selected_tree(
                store,
                read_tree(store, &entry.object.id)?.entries,
                children,
                selectors,
            )?;
            entry.object = ObjectRef::new(ObjectKind::Tree, tree.id);
            entries.push(entry);
        } else {
            entries.push(entry);
        }
    }
    entries.extend(current.into_values());
    write_tree(store, entries)
}

/// Selection eligibility for a workspace-relative path, without reading its contents.
/// `included` describes the current selection rules; a missing path can be eligible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePathExplanation {
    pub path: PathBuf,
    pub included: bool,
    pub tracked: bool,
    pub ignored: bool,
    pub protected: bool,
    pub metadata: bool,
    pub exists: Option<bool>,
    pub is_directory: Option<bool>,
    pub supported_type: Option<bool>,
}

/// Explains selection using the same rules as workspace snapshotting.
/// Only configuration, filesystem metadata, and baseline snapshot/tree objects are read.
/// Symlink ancestors are rejected instead of following them outside the workspace.
pub fn explain_workspace_path(
    store: &impl ObjectStore,
    root: impl AsRef<Path>,
    baseline: Option<&ObjectId>,
    path: impl AsRef<Path>,
) -> SnapshotResult<WorkspacePathExplanation> {
    let mut selection = WorkspaceSelection::load(store, root.as_ref(), baseline)?;
    explain_path(&mut selection, path.as_ref())
}

fn normalize_selector(path: &Path) -> SnapshotResult<PathBuf> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir => {}
            _ => {
                return Err(config_error(
                    path,
                    "path must be workspace-relative without parent components",
                ))
            }
        }
    }
    Ok(relative)
}

fn explain_path(
    selection: &mut WorkspaceSelection,
    path: &Path,
) -> SnapshotResult<WorkspacePathExplanation> {
    let relative = normalize_selector(path)?;
    let metadata = relative.components().next().is_some_and(|component| {
        component.as_os_str() == ".sorrel" || component.as_os_str() == ".git"
    });
    if metadata {
        // Root metadata names are excluded before type inspection in snapshotting.
        // Their descendants may be unreachable through a Git worktree pointer file
        // or lead outside the workspace through a metadata symlink.
        return Ok(WorkspacePathExplanation {
            tracked: selection
                .tracked
                .iter()
                .any(|path| path.starts_with(&relative)),
            protected: selection.is_protected(&relative),
            path: relative,
            included: false,
            ignored: false,
            metadata: true,
            exists: None,
            is_directory: None,
            supported_type: None,
        });
    }
    let mut protected = selection.is_protected(&relative);
    let mut parent = PathBuf::new();
    for component in relative.parent().unwrap_or(Path::new("")).components() {
        parent.push(component.as_os_str());
        let absolute = selection.root.join(&parent);
        match fs::symlink_metadata(&absolute) {
            Ok(info) if !info.is_dir() => {
                return Err(SnapshotError::UnsupportedFileType { path: absolute })
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(config_io(&absolute, error)),
        }
        protected |= selection.is_protected(&parent);
        selection.allows(&parent, true)?;
    }
    let absolute = selection.root.join(&relative);
    let info = match fs::symlink_metadata(&absolute) {
        Ok(info) => Some(info),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(config_io(&absolute, error)),
    };
    let is_directory = info.as_ref().is_some_and(|info| info.is_dir());
    let supported_type = info
        .as_ref()
        .is_none_or(|info| info.is_dir() || info.is_file());
    let tracked = selection.tracked.contains(&relative)
        || (is_directory
            && selection
                .tracked
                .iter()
                .any(|path| path.starts_with(&relative)));
    let ignored = !metadata && selection.is_ignored(&relative, is_directory)?;
    let included =
        !metadata && !protected && supported_type && selection.allows(&relative, is_directory)?;
    Ok(WorkspacePathExplanation {
        path: if relative.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            relative
        },
        included,
        tracked,
        ignored,
        protected,
        metadata,
        exists: Some(info.is_some()),
        is_directory: Some(is_directory),
        supported_type: Some(supported_type),
    })
}

pub(crate) struct WorkspaceSelection {
    root: PathBuf,
    selectors: Option<BTreeSet<PathBuf>>,
    tracked: BTreeSet<PathBuf>,
    tracked_directories: BTreeSet<PathBuf>,
    protected: BTreeSet<PathBuf>,
    rules: BTreeMap<PathBuf, Gitignore>,
    ignored_directories: BTreeSet<PathBuf>,
}

impl WorkspaceSelection {
    pub(crate) fn load(
        store: &impl ObjectStore,
        root: &Path,
        baseline: Option<&ObjectId>,
    ) -> SnapshotResult<Self> {
        let root = fs::canonicalize(root).map_err(|error| config_io(root, error))?;
        let mut selection = Self {
            root,
            selectors: None,
            tracked: BTreeSet::new(),
            tracked_directories: BTreeSet::new(),
            protected: BTreeSet::new(),
            rules: BTreeMap::new(),
            ignored_directories: BTreeSet::new(),
        };
        selection.load_providers()?;
        if let Some(id) = baseline {
            let snapshot = read_snapshot(store, id)?;
            collect_tracked(
                store,
                &snapshot.root_tree.id,
                &mut selection.tracked,
                &mut selection.tracked_directories,
            )?;
        }
        Ok(selection)
    }

    fn validate_tracked_paths(&self) -> SnapshotResult<()> {
        for path in &self.tracked {
            if self.is_protected(path) {
                return Err(SnapshotError::TrackedSecret { path: path.clone() });
            }
        }
        Ok(())
    }

    pub(crate) fn is_protected(&self, path: &Path) -> bool {
        // Match conservatively on case-sensitive hosts too, so a workspace
        // cannot expose credentials when moved to a case-insensitive filesystem.
        let lowercase = path.to_string_lossy().to_ascii_lowercase();
        let path = Path::new(&lowercase);
        is_protected_environment_path(path)
            || self
                .protected
                .iter()
                .any(|protected| path.starts_with(protected))
    }

    pub(crate) fn allows(&mut self, path: &Path, is_dir: bool) -> SnapshotResult<bool> {
        if self.selectors.as_ref().is_some_and(|selectors| {
            !selectors.iter().any(|selected| {
                path.starts_with(selected) || (is_dir && selected.starts_with(path))
            })
        }) || self.is_protected(path)
        {
            return Ok(false);
        }
        if self.tracked.contains(path) {
            return Ok(true);
        }
        let tracked_directory =
            is_dir && self.tracked.iter().any(|tracked| tracked.starts_with(path));
        let ignored = self.is_ignored(path, is_dir)?;
        if ignored && is_dir {
            self.ignored_directories.insert(path.to_owned());
        }
        Ok(!ignored || tracked_directory)
    }

    fn is_ignored(&mut self, path: &Path, is_dir: bool) -> SnapshotResult<bool> {
        if self
            .ignored_directories
            .iter()
            .any(|directory| path.starts_with(directory))
        {
            return Ok(true);
        }
        let mut ignored = false;
        let directories: Vec<_> = path.parent().unwrap_or(Path::new("")).ancestors().collect();
        for directory in directories.into_iter().rev() {
            if !self.rules.contains_key(directory) {
                let root = self.root.join(directory);
                let mut builder = GitignoreBuilder::new(&root);
                for name in [".gitignore", ".sorrelignore"] {
                    let file = root.join(name);
                    if read_optional(&file)?.is_some() {
                        if let Some(error) = builder.add(&file) {
                            return Err(config_error(&file, error));
                        }
                    }
                }
                let rules = builder
                    .build()
                    .map_err(|error| config_error(&root, error))?;
                self.rules.insert(directory.to_owned(), rules);
            }
            let matched =
                self.rules[directory].matched_path_or_any_parents(self.root.join(path), is_dir);
            if !matched.is_none() {
                ignored = matched.is_ignore();
            }
        }
        Ok(ignored)
    }

    fn load_providers(&mut self) -> SnapshotResult<()> {
        let yaml = self.root.join("sorrel.secrets.yml");
        if let Some(bytes) = read_optional(&yaml)? {
            self.add_provider_config(Path::new("sorrel.secrets.yml"), &bytes)?;
        }
        let registry = self.root.join(".sorrel/secrets");
        match fs::read_dir(&registry) {
            Ok(entries) => {
                for entry in entries {
                    let path = entry.map_err(|error| config_io(&registry, error))?.path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                    {
                        let bytes = fs::read(&path).map_err(|error| config_io(&path, error))?;
                        let config: Value = serde_json::from_slice(&bytes)
                            .map_err(|error| config_error(&path, error))?;
                        self.json_providers(&config)?;
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(config_io(&registry, error)),
        }
        let toml_path = self.root.join("secretspec.toml");
        if let Some(bytes) = read_optional(&toml_path)? {
            self.add_provider_config(Path::new("secretspec.toml"), &bytes)?;
        }
        Ok(())
    }

    pub(crate) fn empty(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            selectors: None,
            tracked: BTreeSet::new(),
            tracked_directories: BTreeSet::new(),
            protected: BTreeSet::new(),
            rules: BTreeMap::new(),
            ignored_directories: BTreeSet::new(),
        }
    }

    pub(crate) fn add_provider_config(&mut self, path: &Path, bytes: &[u8]) -> SnapshotResult<()> {
        let file = self.root.join(path);
        let config: Value = if path == Path::new("secretspec.toml") {
            let text = std::str::from_utf8(bytes).map_err(|error| config_error(&file, error))?;
            toml::from_str(text).map_err(|error| config_error(&file, error))?
        } else {
            serde_yaml_ng::from_slice(bytes).map_err(|error| config_error(&file, error))?
        };
        self.json_providers(&config)?;
        if let Some(imports) = config.pointer("/localDev/import/envFiles") {
            let imports = imports
                .as_array()
                .ok_or_else(|| config_error(&file, "localDev.import.envFiles must be an array"))?;
            for import in imports {
                let path = import
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.trim().is_empty())
                    .ok_or_else(|| {
                        config_error(&file, "localDev import requires a non-empty file path")
                    })?;
                self.protect_path(self.root.join(path))?;
            }
        }
        Ok(())
    }

    fn json_providers(&mut self, value: &Value) -> SnapshotResult<()> {
        match value {
            Value::String(provider) => self.protect_provider(provider)?,
            Value::Array(values) => {
                for value in values {
                    self.json_providers(value)?;
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    self.json_providers(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn protect_provider(&mut self, provider: &str) -> SnapshotResult<()> {
        let Some((scheme, _)) = provider.split_once(':') else {
            return Ok(());
        };
        if !scheme.eq_ignore_ascii_case("dotenv") {
            return Ok(());
        }
        let uri = url::Url::parse(provider).map_err(|error| config_error(&self.root, error))?;
        let decode = |value: &str| {
            percent_encoding::percent_decode_str(value)
                .decode_utf8()
                .map(|value| value.into_owned())
                .map_err(|error| config_error(&self.root, error))
        };
        let host = uri.host_str().map(decode).transpose()?;
        let path = decode(uri.path())?;
        // Match SecretSpec's host-plus-path interpretation for dotenv URIs.
        let path = if !path.is_empty() && path != "/" {
            format!("{}{path}", host.unwrap_or_default())
        } else {
            host.unwrap_or_else(|| ".env".to_owned())
        };
        let path = if let Some(rest) = path.strip_prefix("~/") {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(rest))
                .ok_or_else(|| {
                    config_error(&self.root, "cannot resolve dotenv home path without HOME")
                })?
        } else {
            self.root.join(path)
        };
        self.protect_path(path)
    }

    fn protect_path(&mut self, path: PathBuf) -> SnapshotResult<()> {
        let mut paths = vec![normalize(&path)];
        // Providers follow symlinks. Protect both the configured spelling and
        // its actual file, including platform-normalized absolute prefixes.
        match fs::canonicalize(&path) {
            Ok(actual) => paths.push(actual),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(config_io(&path, error)),
        }
        let lowercase_root = PathBuf::from(self.root.to_string_lossy().to_ascii_lowercase());
        for path in paths {
            let lowercase_path = PathBuf::from(path.to_string_lossy().to_ascii_lowercase());
            if let Ok(relative) = lowercase_path.strip_prefix(&lowercase_root) {
                if relative.as_os_str().is_empty() {
                    return Err(config_error(&path, "secret provider must name a file"));
                }
                self.protected.insert(relative.to_owned());
            }
        }
        Ok(())
    }
}

pub(crate) fn is_protected_environment_path(path: &Path) -> bool {
    path.components().any(|component| {
        component.as_os_str().to_str().is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == ".env" || (name.starts_with(".env.") && name != ".env.example")
        })
    })
}

fn collect_tracked(
    store: &impl ObjectStore,
    tree: &ObjectId,
    paths: &mut BTreeSet<PathBuf>,
    directories: &mut BTreeSet<PathBuf>,
) -> SnapshotResult<()> {
    for entry in read_tree(store, tree)?.entries {
        match entry.entry_type {
            EntryType::File => {
                paths.insert(entry.path);
            }
            EntryType::Directory => {
                directories.insert(entry.path);
                collect_tracked(store, &entry.object.id, paths, directories)?;
            }
        }
    }
    Ok(())
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}

fn read_optional(path: &Path) -> SnapshotResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(config_io(path, error)),
    }
}

fn config_io(path: &Path, source: io::Error) -> SnapshotError {
    SnapshotError::Io {
        path: path.to_owned(),
        source,
    }
}

fn config_error(path: &Path, message: impl std::fmt::Display) -> SnapshotError {
    SnapshotError::WorkspaceConfiguration {
        path: path.to_owned(),
        message: message.to_string(),
    }
}
