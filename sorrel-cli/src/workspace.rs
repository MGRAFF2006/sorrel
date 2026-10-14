//! Explicit, workspace-local format opt-in and fixed-lane directory bindings.
use crate::repo::{self, Head};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const SHARED_VERSION: &str = "sorrel.workspace.shared.v1";
pub const LINK_VERSION: &str = "sorrel.workspace.link.v1";
static LAYOUT: OnceLock<Layout> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub root: PathBuf,
    pub lane: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub common: PathBuf,
    pub id: String,
    pub binding: Binding,
    pub repo_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Link {
    schema_version: String,
    repo_id: String,
    workspace_id: String,
    common_dir: PathBuf,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn current() -> Option<&'static Layout> {
    LAYOUT.get()
}
pub fn common_dir() -> PathBuf {
    current().map_or_else(repo::sorrel_dir, |layout| layout.common.clone())
}

/// Resolve and validate identity before the command can create even a lock file.
pub fn configure() -> io::Result<()> {
    let local = repo::sorrel_dir();
    let Some(value) = repo::load_manifest_at(&local.join(repo::MANIFEST_FILE))? else {
        if local.join(repo::HEAD_FILE).exists() {
            return Err(invalid("missing workspace manifest beside an existing HEAD; refusing to reinitialize or detach its identity"));
        }
        return Ok(());
    };
    if value["schemaVersion"] == repo::PROTOCOL_VERSION {
        return Ok(());
    }
    let root = fs::canonicalize(std::env::current_dir()?)?;
    let layout = if value["schemaVersion"] == LINK_VERSION {
        let link: Link = serde_json::from_value(value)?;
        let manifest = shared_manifest(&link.common_dir)?;
        let layout = registered_layout(&link.common_dir, &manifest, &link.workspace_id)?;
        if layout.repo_id != link.repo_id || layout.binding.root != root {
            return Err(invalid(
                "workspace link identity or location disagrees with registration",
            ));
        }
        layout
    } else {
        let common = root.join(repo::SORREL_DIR);
        let manifest = shared_manifest(&common)?;
        let layout = registered_layout(&common, &manifest, "default")?;
        if layout.binding.root != root {
            return Err(invalid("default workspace moved"));
        }
        layout
    };
    validate_link(&layout)?;
    if current().is_some_and(|old| old != &layout) {
        return Err(invalid("workspace layout changed"));
    }
    let _ = LAYOUT.set(layout);
    Ok(())
}

pub(crate) fn shared_manifest(common: &Path) -> io::Result<Value> {
    canonical_directory(common)?;
    repo::validate_metadata_path(common, Path::new(repo::MANIFEST_FILE))?;
    let manifest = repo::load_manifest_at(&common.join(repo::MANIFEST_FILE))?
        .ok_or_else(|| invalid("missing common workspace manifest"))?;
    if manifest["schemaVersion"] != SHARED_VERSION || manifest["kind"] != "Workspace" {
        return Err(invalid(
            "common directory is not an enabled shared workspace",
        ));
    }
    let repo_id = manifest["repoId"]
        .as_str()
        .filter(|id| repo::valid_component(id))
        .ok_or_else(|| invalid("invalid shared repository identity"))?;
    let _ = repo_id;
    let bindings = bindings(&manifest)?;
    if !bindings.contains_key("default") {
        return Err(invalid("missing default workspace binding"));
    }
    let mut roots: Vec<&Path> = Vec::new();
    let mut lanes = std::collections::BTreeSet::new();
    for (id, binding) in &bindings {
        if !repo::valid_component(id)
            || !repo::valid_component(&binding.lane)
            || !binding.root.is_absolute()
            || !lanes.insert(&binding.lane)
            || roots
                .iter()
                .any(|root| binding.root.starts_with(root) || root.starts_with(&binding.root))
        {
            return Err(invalid(
                "invalid, overlapping, or competing workspace bindings",
            ));
        }
        roots.push(&binding.root);
    }
    if bindings["default"].root.join(repo::SORREL_DIR) != common {
        return Err(invalid("common directory disagrees with default binding"));
    }
    Ok(manifest)
}
fn bindings(manifest: &Value) -> io::Result<BTreeMap<String, Binding>> {
    serde_json::from_value(manifest["workspaces"].clone()).map_err(io::Error::other)
}
pub(crate) fn registered_layout(common: &Path, manifest: &Value, id: &str) -> io::Result<Layout> {
    let binding = bindings(manifest)?
        .remove(id)
        .ok_or_else(|| invalid("unregistered workspace"))?;
    Ok(Layout {
        common: common.to_owned(),
        id: id.to_owned(),
        binding,
        repo_id: manifest["repoId"]
            .as_str()
            .ok_or_else(|| invalid("missing repoId"))?
            .to_owned(),
    })
}
fn canonical_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute() || fs::canonicalize(path)? != path || !path.is_dir() {
        return Err(invalid(
            "workspace directory is missing, moved, or contains a symlink",
        ));
    }
    Ok(())
}
/// Validate both sides of registration on every publication/recovery. Never recreate a missing root.
pub(crate) fn validate_link(layout: &Layout) -> io::Result<()> {
    canonical_directory(&layout.binding.root)?;
    let local = layout.binding.root.join(repo::SORREL_DIR);
    canonical_directory(&local)?;
    repo::validate_metadata_path(&local, Path::new(repo::MANIFEST_FILE))?;
    let manifest = shared_manifest(&layout.common)?;
    if registered_layout(&layout.common, &manifest, &layout.id)? != *layout {
        return Err(invalid("stale workspace registration"));
    }
    if layout.id != "default" {
        let link: Link = serde_json::from_slice(&fs::read(local.join(repo::MANIFEST_FILE))?)?;
        if link
            != (Link {
                schema_version: LINK_VERSION.to_owned(),
                repo_id: layout.repo_id.clone(),
                workspace_id: layout.id.clone(),
                common_dir: layout.common.clone(),
            })
        {
            return Err(invalid("invalid or stale workspace link"));
        }
    }
    Ok(())
}

pub fn validate_current_head() -> io::Result<()> {
    if let Some(layout) = current() {
        validate_link(layout)?;
        let local = layout.binding.root.join(repo::SORREL_DIR);
        repo::validate_metadata_path(&local, Path::new(repo::HEAD_FILE))?;
        repo::validate_metadata_path(
            &layout.common,
            &Path::new(repo::HEADS_DIR).join(&layout.binding.lane),
        )?;
        let head = repo::load_head()?.ok_or_else(|| invalid("missing workspace HEAD"))?;
        if head.lane != layout.binding.lane
            || repo::load_lane_head(&head.lane)?.as_deref() != Some(&head.snapshot)
        {
            return Err(invalid("workspace HEAD disagrees with its owned lane"));
        }
    }
    Ok(())
}
pub fn check_head_lane(lane: &str) -> io::Result<()> {
    if let Some(layout) = current() {
        validate_link(layout)?;
        if lane != layout.binding.lane {
            return Err(invalid(
                "workspace is pinned to its owned lane; switching is unavailable",
            ));
        }
    }
    Ok(())
}
pub fn check_lane_writer(lane: &str) -> io::Result<()> {
    if let Some(layout) = current() {
        validate_link(layout)?;
        let manifest = shared_manifest(&layout.common)?;
        if bindings(&manifest)?
            .iter()
            .any(|(id, binding)| binding.lane == lane && id != &layout.id)
        {
            return Err(invalid(
                "lane is owned by another workspace; foreign publication refused",
            ));
        }
    }
    Ok(())
}

/// No filesystem or manifest changes in preview. Apply backs up original metadata before the format gate changes.
pub fn enable(apply: bool) -> io::Result<Value> {
    if current().is_some() {
        return Err(invalid("workspace sharing is already enabled"));
    }
    let root = fs::canonicalize(std::env::current_dir()?)?;
    let common = root.join(repo::SORREL_DIR);
    canonical_directory(&common)?;
    let mut manifest =
        repo::load_manifest()?.ok_or_else(|| invalid("initialize the workspace first"))?;
    if manifest["kind"] != "Workspace"
        || manifest["repoId"]
            .as_str()
            .is_none_or(|id| !repo::valid_component(id))
    {
        return Err(invalid("invalid standalone repository identity"));
    }
    preflight(&common)?;
    let head: Head = serde_json::from_slice(&fs::read(common.join(repo::HEAD_FILE))?)?;
    if !repo::valid_component(&head.lane) || !repo::valid_digest(&head.snapshot) {
        return Err(invalid("invalid standalone HEAD"));
    }
    let store = sorrel_core::FileObjectStore::open_existing(&common).map_err(io::Error::other)?;
    let snapshot = sorrel_core::parse_object_id_hex(&head.snapshot).map_err(io::Error::other)?;
    sorrel_core::validate_workspace_snapshot_secrets(
        &store,
        &root,
        &snapshot,
        &common.join(repo::SECRETS_DIR),
    )
    .map_err(io::Error::other)?;
    if repo::load_lane_head(&head.lane)?.as_deref() != Some(&head.snapshot) {
        return Err(invalid(
            "HEAD and lane head disagree; explicit repair is required before enabling",
        ));
    }
    let preview = json!({"olderClients": "refused by workspace schema version", "pinnedLane": head.lane,
        "shared": ["objects", "lane heads", "changes index", "grants", "secret references", "remotes"],
        "local": ["HEAD", "MERGE_STATE", "stat cache", "operations", "checkpoints"],
        "git": "default directory only", "backup": "original mutable metadata, excluding objects, tmp and write.lock",
        "recovery": "metadata publication only; no filesystem rollback", "format": SHARED_VERSION});
    if !apply {
        return Ok(
            json!({"command":"workspace enable", "mocked":false, "status":"preview", "compatibility":preview}),
        );
    }
    // Backup uses the existing synced-file + directory barrier/atomic-rename primitives.
    let backup = backup_metadata(&common)?;
    manifest["schemaVersion"] = json!(SHARED_VERSION);
    manifest["workspaces"] = json!({"default": Binding { root, lane: head.lane }});
    // Last and only original metadata change: a failed preflight/backup leaves legacy format intact.
    repo::write_json_atomic(&common.join(repo::MANIFEST_FILE), &manifest)?;
    Ok(
        json!({"command":"workspace enable", "mocked":false,"status":"enabled", "backup":backup,"compatibility":preview}),
    )
}

fn metadata_files(root: &Path, relative: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    repo::validate_metadata_path(root, relative)?;
    for entry in fs::read_dir(root.join(relative))? {
        let path = relative.join(entry?.file_name());
        if relative.as_os_str().is_empty()
            && matches!(
                path.to_str(),
                Some("objects" | "tmp" | "write.lock" | "workspace-backups")
            )
        {
            continue;
        }
        repo::validate_metadata_path(root, &path)?;
        let metadata = fs::symlink_metadata(root.join(&path))?;
        if metadata.is_dir() {
            metadata_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push(path);
        } else {
            return Err(invalid("unsupported mutable metadata file type"));
        }
    }
    Ok(())
}
fn preflight(common: &Path) -> io::Result<()> {
    if repo::merge_state_path().exists() {
        return Err(invalid("finish or abort the pending merge before enabling"));
    }
    if common.join("metadata-transaction.json").exists() {
        return Err(invalid("recover pending metadata before enabling"));
    }
    if !common.join(repo::HEADS_DIR).is_dir() {
        return Err(invalid("lane heads must exist before enabling"));
    }
    let mut files = Vec::new();
    metadata_files(common, Path::new(""), &mut files)?;
    let store = sorrel_core::FileObjectStore::open_existing(common).map_err(io::Error::other)?;
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(common.join(repo::HEADS_DIR))? {
        let path = entry?.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(repo::valid_component)
        {
            return Err(invalid("invalid lane ref"));
        }
        let relative = Path::new(repo::HEADS_DIR).join(path.file_name().unwrap());
        let value = repo::parse_ref_payload(&relative, &fs::read(path)?)?;
        let id = value["snapshot"]
            .as_str()
            .ok_or_else(|| invalid("invalid lane snapshot"))?;
        snapshots.push(id.to_owned());
    }
    for (_, operation) in repo::load_operations()? {
        for state in [&operation.before, &operation.after] {
            snapshots.extend(state.head.iter().map(|head| head.snapshot.clone()));
            snapshots.extend(state.lane_heads.values().filter_map(Clone::clone));
        }
    }
    for checkpoint in repo::load_checkpoints()? {
        snapshots.push(checkpoint.head.snapshot);
    }
    let index = match fs::read_to_string(common.join(repo::CHANGES_INDEX_FILE)) {
        Ok(index) => index,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    repo::validate_changes_index(common, index.as_bytes())?;
    let mut changes = Vec::new();
    for line in index.lines() {
        let value: Value = serde_json::from_str(line)?;
        snapshots.push(
            value["snapshot"]
                .as_str()
                .ok_or_else(|| invalid("invalid changes index"))?
                .to_owned(),
        );
        changes.push(
            value["change"]
                .as_str()
                .ok_or_else(|| invalid("invalid changes index"))?
                .to_owned(),
        );
    }
    repo::flush_core_references(
        common,
        &snapshots.iter().map(String::as_str).collect::<Vec<_>>(),
        &changes.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    for id in snapshots {
        let id = sorrel_core::parse_object_id_hex(&id).map_err(io::Error::other)?;
        sorrel_core::validate_snapshot_restore(&store, &id).map_err(io::Error::other)?;
    }
    for lane in repo::list_registry_entries(repo::LANES_DIR)? {
        let id = lane["id"]
            .as_str()
            .filter(|id| repo::valid_component(id))
            .ok_or_else(|| invalid("invalid lane registry"))?;
        if !common.join(repo::HEADS_DIR).join(id).is_file() {
            return Err(invalid("lane registry has no authoritative head"));
        }
    }
    Ok(())
}
fn backup_metadata(common: &Path) -> io::Result<PathBuf> {
    repo::validate_metadata_path(common, Path::new("workspace-backups"))?;
    let parent = common.join("workspace-backups");
    fs::create_dir_all(&parent)?;
    let id = repo::generate_repo_id();
    let staging = parent.join(format!("{id}.incomplete"));
    fs::create_dir(&staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))?;
    }
    let mut files = Vec::new();
    metadata_files(common, Path::new(""), &mut files)?;
    let mut hashes = BTreeMap::new();
    for path in files {
        let bytes = fs::read(common.join(&path))?;
        let destination = staging.join("metadata").join(&path);
        repo::write_bytes_atomic(&destination, &bytes)?;
        hashes.insert(path, sorrel_core::ObjectId::for_bytes(&bytes).to_hex());
    }
    repo::write_json_atomic(
        &staging.join("backup.json"),
        &json!({"version":1, "files":hashes,
        "restore":"With all clients stopped, restore metadata/ over the original .sorrel. This restores pre-enable refs, not later linked-directory work. Keep immutable objects. Do not restore while linked directories are in use."}),
    )?;
    sorrel_core::durability::flush_root_ancestors(&staging)?;
    let backup = parent.join(id);
    fs::rename(&staging, &backup)?;
    sorrel_core::durability::flush_directory_chain(&parent, common)?;
    Ok(backup)
}

pub fn list() -> io::Result<Value> {
    let layout = current().ok_or_else(|| {
        invalid("sharing is disabled; run workspace enable for compatibility preview")
    })?;
    let manifest = shared_manifest(&layout.common)?;
    Ok(
        json!({"command":"workspace list","mocked":false,"repoId":layout.repo_id,"currentWorkspace":layout.id,"workspaces":bindings(&manifest)?}),
    )
}
pub fn add(lane: &str, destination: &Path) -> io::Result<Value> {
    let layout = current().ok_or_else(|| {
        invalid("sharing is disabled; run workspace enable then workspace enable --apply")
    })?;
    validate_link(layout)?;
    let mut manifest = shared_manifest(&layout.common)?;
    let mut workspaces = bindings(&manifest)?;
    if !repo::valid_component(lane) || !repo::lane_exists(lane) {
        return Err(invalid("lane does not exist"));
    }
    if workspaces.values().any(|binding| binding.lane == lane) {
        return Err(invalid("lane is already owned by a workspace"));
    }
    let name = destination
        .file_name()
        .filter(|name| *name != "." && *name != "..")
        .ok_or_else(|| invalid("destination must be a new directory"))?;
    let parent = fs::canonicalize(
        destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let root = parent.join(name);
    if fs::symlink_metadata(&root).is_ok()
        || workspaces
            .values()
            .any(|binding| root.starts_with(&binding.root) || binding.root.starts_with(&root))
    {
        return Err(invalid(
            "destination exists or overlaps a registered workspace",
        ));
    }
    let snapshot =
        repo::load_lane_head(lane)?.ok_or_else(|| invalid("lane has no recorded head"))?;
    let id = sorrel_core::parse_object_id_hex(&snapshot).map_err(io::Error::other)?;
    let store =
        sorrel_core::FileObjectStore::open_existing(&layout.common).map_err(io::Error::other)?;
    sorrel_core::validate_snapshot_restore(&store, &id).map_err(io::Error::other)?;
    sorrel_core::validate_workspace_snapshot_secrets(
        &store,
        &layout.binding.root,
        &id,
        &repo::registry_dir(repo::SECRETS_DIR),
    )
    .map_err(io::Error::other)?;
    sorrel_core::validate_workspace_snapshot_secrets_at(
        &store,
        &root,
        &id,
        &repo::registry_dir(repo::SECRETS_DIR),
    )
    .map_err(io::Error::other)?;
    repo::flush_core_references(&layout.common, &[&snapshot], &[])?;
    let workspace_id = repo::generate_repo_id().replace("repo_", "workspace_");
    // Registration is last. Interrupted creation leaves an explicitly unregistered directory.
    fs::create_dir(&root)?;
    let local = root.join(repo::SORREL_DIR);
    fs::create_dir(&local)?;
    fs::create_dir(local.join("tmp"))?;
    let link = Link {
        schema_version: LINK_VERSION.to_owned(),
        repo_id: layout.repo_id.clone(),
        workspace_id: workspace_id.clone(),
        common_dir: layout.common.clone(),
    };
    repo::write_json_atomic(
        &local.join(repo::MANIFEST_FILE),
        &serde_json::to_value(link)?,
    )?;
    sorrel_core::restore_snapshot_to_directory(&store, &id, &root).map_err(io::Error::other)?;
    repo::write_json_atomic(
        &local.join(repo::HEAD_FILE),
        &serde_json::to_value(Head {
            lane: lane.to_owned(),
            snapshot,
        })?,
    )?;
    sorrel_core::durability::flush_root_ancestors(&local)?;
    workspaces.insert(
        workspace_id.clone(),
        Binding {
            root: root.clone(),
            lane: lane.to_owned(),
        },
    );
    manifest["workspaces"] = serde_json::to_value(workspaces)?;
    repo::write_json_atomic(&layout.common.join(repo::MANIFEST_FILE), &manifest)?;
    Ok(
        json!({"command":"workspace add","mocked":false,"status":"created","workspaceId":workspace_id,"lane":lane,"path":root}),
    )
}
