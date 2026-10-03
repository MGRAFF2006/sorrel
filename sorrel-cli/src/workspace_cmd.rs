//! Isolated working directories with explicit import into their owner's lanes.

use crate::{agent_cmd, command_error, repo, tracking, CommandOutput};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use sorrel_core::{FileObjectStore, LaneOptions, ObjectId, ObjectStore, Principal, Visibility};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Create a new isolated working directory and lane for an agent.
    Create(CreateArgs),
    /// List workspaces with their current local heads.
    List,
    /// Inspect recorded agent work without importing or integrating it.
    Review { id: String },
    /// Bring an agent workspace's recorded work into the active lane.
    Integrate {
        id: String,
        /// Require the worker tip to match a previously reviewed snapshot.
        #[arg(long)]
        snapshot: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub agent: String,
    #[arg(long)]
    pub task: Option<String>,
}

fn store_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn record_path(id: &str) -> io::Result<PathBuf> {
    agent_cmd::valid_id(id)?;
    Ok(repo::sorrel_dir()
        .join("workspaces")
        .join(format!("{id}.json")))
}

fn read_json(path: &Path) -> io::Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "workspace record is not a regular file",
        ));
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn history_index(path: &Path) -> io::Result<BTreeMap<ObjectId, ObjectId>> {
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error),
    };
    let mut index = BTreeMap::new();
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)?;
        let snapshot = object_id(&value, "snapshot")?;
        let change = object_id(&value, "change")?;
        if index
            .insert(snapshot, change)
            .is_some_and(|previous| previous != change)
        {
            return Err(command_error(
                "invalid_data",
                "conflicting history index mappings",
            ));
        }
    }
    Ok(index)
}

fn object_id(value: &Value, key: &str) -> io::Result<ObjectId> {
    value[key]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", format!("missing {key}")))?
        .parse()
        .map_err(store_error)
}

fn index_bytes(index: &BTreeMap<ObjectId, ObjectId>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for (snapshot, change) in index {
        serde_json::to_writer(
            &mut bytes,
            &json!({"snapshot":snapshot.to_hex(),"change":change.to_hex()}),
        )?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn history_objects(
    store: &FileObjectStore,
    index: &BTreeMap<ObjectId, ObjectId>,
    reachable: &BTreeSet<ObjectId>,
) -> io::Result<BTreeSet<ObjectId>> {
    let mut objects = BTreeSet::new();
    let mut queue = Vec::new();
    for (snapshot, change_id) in index {
        if !reachable.contains(snapshot) {
            continue;
        }
        let change = sorrel_core::read_change(store, change_id).map_err(store_error)?;
        if change.resulting_snapshot.id != *snapshot {
            return Err(command_error(
                "invalid_data",
                "history mapping does not match Change result",
            ));
        }
        queue.push(*change_id);
    }
    while let Some(id) = queue.pop() {
        if !objects.insert(id) {
            continue;
        }
        let change = sorrel_core::read_change(store, &id).map_err(store_error)?;
        if !reachable.contains(&change.resulting_snapshot.id) {
            return Err(command_error(
                "invalid_data",
                "Change history escapes workspace ancestry",
            ));
        }
        if !reachable.contains(&change.base_snapshot.id) {
            // Git's oldest imported commit diffs against a synthetic empty
            // baseline, which is not a parent of that root snapshot.
            let result = sorrel_core::read_snapshot(store, &change.resulting_snapshot.id)
                .map_err(store_error)?;
            let baseline =
                sorrel_core::read_snapshot(store, &change.base_snapshot.id).map_err(store_error)?;
            sorrel_core::validate_snapshot(store, &baseline.id).map_err(store_error)?;
            let tree =
                sorrel_core::read_tree(store, &baseline.root_tree.id).map_err(store_error)?;
            if !result.parents.is_empty()
                || !baseline.parents.is_empty()
                || !change.parent_changes.is_empty()
                || baseline.repo != result.repo
                || !tree.entries.is_empty()
            {
                return Err(command_error(
                    "invalid_data",
                    "Change history escapes workspace ancestry",
                ));
            }
            objects
                .extend(sorrel_core::collect_closure(store, &[baseline.id]).map_err(store_error)?);
        }
        let actual = sorrel_core::snapshot_diff(
            store,
            &change.base_snapshot.id,
            &change.resulting_snapshot.id,
        )
        .map_err(store_error)?;
        if actual != change.diff || actual.touched_paths() != change.touched_paths {
            return Err(command_error(
                "invalid_data",
                "Change diff does not match its snapshots",
            ));
        }
        queue.extend(change.parent_changes.iter().map(|parent| parent.id));
    }
    Ok(objects)
}

fn sync_workspace(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            sync_workspace(&entry.path())?;
        } else if kind.is_file() {
            fs::File::open(entry.path())?.sync_all()?;
        } else {
            return Err(command_error(
                "invalid_data",
                "unexpected workspace staging entry",
            ));
        }
    }
    sync_directory(path)
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Finish a staged workspace publication after interruption. Call with the owner
/// repository lock held, before accepting another command.
pub fn recover_creation() -> io::Result<()> {
    let journal_path = repo::sorrel_dir().join("WORKSPACE_CREATE");
    let journal = match read_json(&journal_path) {
        Ok(journal) => journal,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let workspace = &journal["workspace"];
    let id = workspace["id"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "workspace journal missing id"))?;
    let record = record_path(id)?;
    let lane = workspace["lane"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "workspace journal missing lane"))?;
    let lane_id: ObjectId = lane.parse().map_err(store_error)?;
    let destination =
        PathBuf::from(workspace["path"].as_str().ok_or_else(|| {
            command_error("invalid_data", "workspace journal missing destination")
        })?);
    let staging = PathBuf::from(
        journal["staging"]
            .as_str()
            .ok_or_else(|| command_error("invalid_data", "workspace journal missing staging"))?,
    );
    let owner = fs::canonicalize(".")?;
    if workspace["owner"].as_str() != owner.to_str()
        || workspace["agentId"].as_str() != Some(id)
        || journal["agent"]["id"].as_str() != Some(id)
        || journal["agent"]["lane"].as_str() != Some(lane)
        || journal["agent"]["workspace"].as_str() != destination.to_str()
        || journal["lane"]["id"].as_str() != Some(lane)
        || !destination.is_absolute()
        || !staging.is_absolute()
        || destination.starts_with(&owner)
        || staging.parent() != destination.parent()
        || !staging
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".sorrel-workspace-"))
    {
        return Err(command_error(
            "invalid_data",
            "workspace creation journal does not match its owner",
        ));
    }
    let base = object_id(workspace, "baseSnapshot")?;
    if journal["lane"]["baseSnapshot"]["id"].as_str() != Some(base.to_hex().as_str())
        || journal["lane"]["headSnapshot"]["id"].as_str() != Some(base.to_hex().as_str())
    {
        return Err(command_error(
            "invalid_data",
            "workspace journal lane does not match base",
        ));
    }
    let store = FileObjectStore::new(repo::object_store_root()).map_err(store_error)?;
    let stored_lane = sorrel_core::read_lane(&store, &lane_id).map_err(store_error)?;
    if stored_lane.base_snapshot.id != base || stored_lane.head_snapshot.id != base {
        return Err(command_error(
            "invalid_data",
            "workspace journal does not match stored lane",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| command_error("invalid_data", "workspace missing parent"))?;
    if fs::canonicalize(parent)? != parent {
        return Err(command_error(
            "invalid_data",
            "workspace destination parent changed",
        ));
    }
    if destination.try_exists()? {
        if fs::symlink_metadata(&destination)?.file_type().is_symlink()
            || read_json(&destination.join(".sorrel/workspace.json"))? != *workspace
        {
            return Err(command_error(
                "already_exists",
                "workspace destination changed; publication journal retained",
            ));
        }
    } else {
        if fs::symlink_metadata(&staging)?.file_type().is_symlink()
            || read_json(&staging.join(".sorrel/workspace.json"))? != *workspace
        {
            return Err(command_error(
                "invalid_data",
                "workspace staging link does not match journal",
            ));
        }
        let store = FileObjectStore::new(staging.join(".sorrel")).map_err(store_error)?;
        sorrel_core::validate_snapshot(&store, &base).map_err(store_error)?;
        fs::rename(&staging, &destination)?;
        sync_directory(
            destination
                .parent()
                .ok_or_else(|| command_error("invalid_data", "workspace missing parent"))?,
        )?;
    }
    if record.try_exists()? && read_json(&record)? != *workspace {
        return Err(command_error(
            "already_exists",
            "workspace owner record changed; publication journal retained",
        ));
    }
    repo::write_registry_entry(repo::LANES_DIR, lane, &journal["lane"])?;
    repo::write_lane_head(lane, &base.to_hex())?;
    repo::write_json_atomic(&record, workspace)?;
    agent_cmd::register_record(&repo::sorrel_dir().join("agents"), &journal["agent"])?;
    fs::remove_file(&journal_path)?;
    sync_directory(&repo::sorrel_dir())
}

/// Identity attached to this checkout. Changing lanes retains the worker identity.
pub fn current_principal() -> io::Result<Principal> {
    let link = match read_json(&repo::sorrel_dir().join("workspace.json")) {
        Ok(link) => link,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Principal::system()),
        Err(error) => return Err(error),
    };
    if link["kind"].as_str() != Some("AgentWorkspace")
        || link["schemaVersion"].as_str() != Some(repo::PROTOCOL_VERSION)
    {
        return Err(command_error(
            "invalid_workspace",
            "invalid worker identity link",
        ));
    }
    let id = link["agentId"]
        .as_str()
        .ok_or_else(|| command_error("invalid_workspace", "worker link missing agentId"))?;
    agent_cmd::valid_id(id)?;
    if link["id"].as_str() != Some(id) {
        return Err(command_error(
            "invalid_workspace",
            "worker link identity does not match",
        ));
    }
    object_id(&link, "lane")?;
    Ok(Principal::new("agent", id, Some(id.to_owned())))
}

pub fn create(args: CreateArgs) -> io::Result<CommandOutput> {
    recover_creation()?;
    let root = fs::canonicalize(".")?;
    if repo::sorrel_dir().join("workspace.json").exists() {
        return Err(command_error(
            "invalid_workspace",
            "create agent workspaces from the owner checkout",
        ));
    }
    let record = record_path(&args.agent)?;
    if record.exists() {
        return Err(command_error(
            "already_exists",
            "agent already has a workspace",
        ));
    }
    let manifest = repo::load_manifest()?
        .ok_or_else(|| command_error("uninitialized", "run `sorrel init` first"))?;
    let repo_id = manifest["repoId"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "manifest missing repoId"))?;
    let head = repo::load_head()?.ok_or_else(|| command_error("invalid_data", "missing HEAD"))?;
    let base: ObjectId = head.snapshot.parse().map_err(store_error)?;
    let requested = if args.path.is_absolute() {
        args.path
    } else {
        root.join(args.path)
    };
    if requested.exists() {
        return Err(command_error(
            "already_exists",
            "workspace destination must be a new directory",
        ));
    }
    let parent = requested
        .parent()
        .ok_or_else(|| command_error("invalid_input", "workspace needs a parent directory"))?;
    fs::create_dir_all(parent)?;
    let parent = fs::canonicalize(parent)?;
    let destination = parent.join(
        requested
            .file_name()
            .ok_or_else(|| command_error("invalid_input", "invalid destination"))?,
    );
    if destination.starts_with(&root) {
        return Err(command_error(
            "invalid_input",
            "create agent workspaces outside the owner working tree",
        ));
    }
    let store = FileObjectStore::new(repo::object_store_root()).map_err(store_error)?;
    sorrel_core::validate_snapshot(&store, &base).map_err(store_error)?;
    if sorrel_core::read_snapshot(&store, &base)
        .map_err(store_error)?
        .repo
        != repo_id
    {
        return Err(command_error(
            "invalid_data",
            "base snapshot belongs to another repository",
        ));
    }
    let agent_state = agent_cmd::load_active(&repo::sorrel_dir().join("agents"))?;
    let _ = tracking::selection_at(&store, &root, Some(&base))?;
    let history = history_index(&repo::changes_index_path())?;
    let reachable = sorrel_core::collect_ancestors(&store, base).map_err(store_error)?;
    let changes = history_objects(&store, &history, &reachable)?;
    let created_at = repo::now_rfc3339();
    let mut lane_options = LaneOptions::new(
        format!("agent/{}", args.agent),
        base,
        base,
        Principal::new("agent", &args.agent, Some(args.agent.clone())),
        Visibility::Private,
    );
    lane_options.created_at = created_at.clone();
    let lane = sorrel_core::create_lane(&store, lane_options).map_err(store_error)?;
    let lane_id = lane.id.to_hex();
    let lane_record = json!({"kind":"Lane", "id":lane_id, "name":lane.name, "baseSnapshot":{"kind":"Snapshot","id":head.snapshot}, "headSnapshot":{"kind":"Snapshot","id":head.snapshot}, "createdAt":created_at});
    let staging = tempfile::Builder::new()
        .prefix(".sorrel-workspace-")
        .tempdir_in(&parent)?;
    let metadata = staging.path().join(repo::SORREL_DIR);
    let worker_store = FileObjectStore::new(&metadata).map_err(store_error)?;
    // ponytail: independent stores copy reachable objects; share a pool if
    // measured workspace-creation cost warrants the additional ownership model.
    worker_store
        .write(&store.read(&lane.id).map_err(store_error)?)
        .map_err(store_error)?;
    let closure = sorrel_core::collect_closure(&store, &[base]).map_err(store_error)?;
    for id in closure.into_iter().chain(changes) {
        worker_store
            .write(&store.read(&id).map_err(store_error)?)
            .map_err(store_error)?;
    }
    repo::write_json_atomic(&metadata.join("manifest.json"), &manifest)?;
    repo::write_json_atomic(
        &metadata.join("HEAD"),
        &json!({"lane":lane_id,"snapshot":head.snapshot}),
    )?;
    repo::write_json_atomic(
        &metadata.join("heads").join(&lane_id),
        &json!({"snapshot":head.snapshot}),
    )?;
    repo::write_json_atomic(
        &metadata.join("lanes").join(format!("{lane_id}.json")),
        &lane_record,
    )?;
    let history: BTreeMap<_, _> = history
        .into_iter()
        .filter(|(snapshot, _)| reachable.contains(snapshot))
        .collect();
    repo::write_bytes_atomic(&metadata.join("changes.index"), &index_bytes(&history)?)?;
    for name in ["tracked.json", "remotes.json"] {
        let source = repo::sorrel_dir().join(name);
        match fs::read(source) {
            Ok(bytes) => repo::write_bytes_atomic(&metadata.join(name), &bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    sorrel_core::restore_snapshot_to_directory(&worker_store, &base, staging.path())
        .map_err(store_error)?;
    let mut workspace = json!({"schemaVersion":repo::PROTOCOL_VERSION,"kind":"AgentWorkspace","id":args.agent,"agentId":args.agent,"lane":lane_id,"baseSnapshot":head.snapshot,"path":destination,"owner":root,"createdAt":created_at});
    let mut agent = agent_state["agents"]
        .as_array()
        .and_then(|agents| agents.iter().find(|agent| agent["id"] == args.agent))
        .cloned()
        .unwrap_or_else(
            || json!({"id":args.agent,"displayName":args.agent,"registeredAt":created_at}),
        );
    agent["lane"] = json!(lane_id);
    agent["workspace"] = json!(destination.to_string_lossy());
    if let Some(task) = args.task {
        if task.is_empty() {
            return Err(command_error("invalid_input", "task must not be empty"));
        }
        workspace["task"] = json!(task);
        agent["task"] = workspace["task"].clone();
    } else if let Some(object) = agent.as_object_mut() {
        object.remove("task");
    }
    repo::write_json_atomic(&metadata.join("workspace.json"), &workspace)?;
    sync_workspace(staging.path())?;
    repo::write_json_atomic(
        &repo::sorrel_dir().join("WORKSPACE_CREATE"),
        &json!({"workspace":workspace,"agent":agent,"lane":lane_record,"staging":staging.path()}),
    )?;
    // The durable journal owns staging after this point; retain it on errors.
    let _staging = staging.keep();
    recover_creation()?;
    Ok(CommandOutput {
        json: json!({"command":"workspace create","status":"created","workspace":workspace}),
        human: format!(
            "Created workspace {} at {}",
            args.agent,
            destination.display()
        ),
    })
}

struct OwnerState {
    repo_id: String,
    ancestors: BTreeSet<ObjectId>,
    history: BTreeMap<ObjectId, ObjectId>,
}

fn owner_state() -> io::Result<OwnerState> {
    let manifest = repo::load_manifest()?
        .ok_or_else(|| command_error("uninitialized", "run `sorrel init` first"))?;
    let repo_id = manifest["repoId"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "manifest missing repoId"))?
        .to_owned();
    let head =
        repo::load_head()?.ok_or_else(|| command_error("invalid_data", "missing owner HEAD"))?;
    let tip: ObjectId = head.snapshot.parse().map_err(store_error)?;
    let store = FileObjectStore::new(repo::object_store_root()).map_err(store_error)?;
    sorrel_core::validate_snapshot(&store, &tip).map_err(store_error)?;
    let ancestors = sorrel_core::collect_ancestors(&store, tip).map_err(store_error)?;
    let history = history_index(&repo::changes_index_path())?;
    // A corrupt owner history must not be presented as an empty work queue.
    history_objects(&store, &history, &ancestors)?;
    Ok(OwnerState {
        repo_id,
        ancestors,
        history,
    })
}

fn validate_workspace(workspace: &Value) -> io::Result<()> {
    let id = workspace["id"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "workspace missing id"))?;
    agent_cmd::valid_id(id)?;
    if workspace["schemaVersion"].as_str() != Some(repo::PROTOCOL_VERSION)
        || workspace["kind"].as_str() != Some("AgentWorkspace")
        || workspace["agentId"].as_str() != Some(id)
        || workspace["owner"].as_str() != fs::canonicalize(".")?.to_str()
        || !workspace["path"].as_str().is_some_and(|path| {
            Path::new(path).is_absolute()
                && !Path::new(path).components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
        })
    {
        return Err(command_error(
            "invalid_workspace",
            "workspace record does not match its owner",
        ));
    }
    let owner = fs::canonicalize(".")?;
    if workspace["path"]
        .as_str()
        .is_some_and(|path| Path::new(path).starts_with(&owner))
    {
        return Err(command_error(
            "invalid_workspace",
            "worker path must stay outside its owner checkout",
        ));
    }
    object_id(workspace, "lane")?;
    object_id(workspace, "baseSnapshot")?;
    Ok(())
}

/// A stable view of recorded work. The lock lives through diff rendering or import.
pub struct Review {
    pub store: FileObjectStore,
    pub base: ObjectId,
    pub tip: ObjectId,
    pub workspace: Value,
    pub overview: Value,
    pub commits: Vec<Value>,
    reachable: BTreeSet<ObjectId>,
    history: BTreeMap<ObjectId, ObjectId>,
    change_objects: BTreeSet<ObjectId>,
    _worker_lock: repo::RepositoryLock,
}

fn inspect(workspace: Value, owner: &OwnerState) -> io::Result<Review> {
    validate_workspace(&workspace)?;
    let path = PathBuf::from(
        workspace["path"]
            .as_str()
            .ok_or_else(|| command_error("invalid_data", "workspace missing path"))?,
    );
    if let Ok(canonical) = fs::canonicalize(&path) {
        if canonical != path || canonical.starts_with(fs::canonicalize(".")?) {
            return Err(command_error(
                "invalid_workspace",
                "worker path changed or aliases the owner checkout",
            ));
        }
    }
    let metadata = path.join(".sorrel");
    // Lock acquisition creates metadata for init. Inspect the existing checkout
    // first so missing or moved workers do not leave phantom repositories.
    for directory in [&path, &metadata] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(command_error(
                    "invalid_workspace",
                    "worker directory is not a regular directory",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "worker directory is missing or moved",
                ));
            }
            Err(error) => return Err(error),
        }
    }
    read_json(&metadata.join("workspace.json"))?;
    let worker_lock = repo::RepositoryLock::acquire(&path)?;
    if read_json(&metadata.join("workspace.json"))? != workspace {
        return Err(command_error(
            "invalid_workspace",
            "workspace link does not match owner record",
        ));
    }
    let manifest = read_json(&metadata.join("manifest.json"))?;
    if manifest["repoId"].as_str() != Some(owner.repo_id.as_str()) {
        return Err(command_error(
            "invalid_workspace",
            "workspace repository does not match",
        ));
    }
    let head = read_json(&metadata.join("HEAD"))?;
    let active_lane = head["lane"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "worker HEAD missing lane"))?;
    let assigned_lane = workspace["lane"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "workspace missing lane"))?;
    let switched = active_lane != assigned_lane;
    let tip = object_id(&head, "snapshot")?;
    let base = object_id(&workspace, "baseSnapshot")?;
    let store = FileObjectStore::new(&metadata).map_err(store_error)?;
    sorrel_core::validate_snapshot(&store, &tip).map_err(store_error)?;
    if sorrel_core::read_snapshot(&store, &tip)
        .map_err(store_error)?
        .repo
        != owner.repo_id
    {
        return Err(command_error(
            "invalid_workspace",
            "worker snapshot belongs to another repository",
        ));
    }
    let reachable = sorrel_core::collect_ancestors(&store, tip).map_err(store_error)?;
    if !reachable.contains(&base) {
        return Err(command_error(
            "invalid_workspace",
            "agent tip is not descended from its recorded base",
        ));
    }
    let history = history_index(&metadata.join("changes.index"))?;
    let change_objects = history_objects(&store, &history, &reachable)?;
    for (snapshot, change) in &history {
        if reachable.contains(snapshot)
            && owner
                .history
                .get(snapshot)
                .is_some_and(|known| known != change)
        {
            return Err(command_error(
                "invalid_data",
                "worker history would replace an owner Change mapping",
            ));
        }
    }
    let recovering = [
        "MERGE_STATE",
        "CHECKOUT_STATE",
        "HEAD_TRANSACTION",
        "WORKSPACE_CREATE",
    ]
    .iter()
    .any(|name| metadata.join(name).exists());
    let selection = tracking::selection_at(&store, &path, Some(&tip))?;
    let working = sorrel_core::materialize_snapshot_filtered_with_stat_cache(
        &store,
        &path,
        None,
        sorrel_core::SnapshotOptions::new(&owner.repo_id),
        |path, directory| Ok(selection.includes(path, directory)),
    )
    .map_err(store_error)?;
    let dirty = !sorrel_core::snapshot_diff(&store, &tip, &working.id)
        .map_err(store_error)?
        .changes
        .is_empty();
    let pending = reachable.difference(&owner.ancestors).count();
    let integrated = owner.ancestors.contains(&tip);
    let mut blockers = Vec::new();
    if recovering {
        blockers.push("recovery_required");
    }
    if switched {
        blockers.push("assigned_lane_changed");
    }
    if dirty {
        blockers.push("dirty_worktree");
    }
    let status = if recovering {
        "recovering"
    } else if switched {
        "switched"
    } else if dirty {
        "dirty"
    } else if integrated {
        "integrated"
    } else {
        "ready"
    };
    let mut overview = workspace.clone();
    overview["headSnapshot"] = json!(tip.to_hex());
    overview["activeLane"] = json!(active_lane);
    overview["dirty"] = json!(dirty);
    overview["recovering"] = json!(recovering);
    overview["pendingSnapshots"] = json!(pending);
    overview["integrated"] = json!(integrated);
    overview["status"] = json!(status);
    overview["blockers"] = json!(blockers);
    overview["readyToIntegrate"] = json!(blockers.is_empty() && pending > 0);

    let original = sorrel_core::collect_ancestors(&store, base).map_err(store_error)?;
    // Parents precede children, including both sides of worker merges.
    let mut queue = vec![(tip, false)];
    let mut visited = BTreeSet::new();
    let mut commits = Vec::new();
    while let Some((id, expanded)) = queue.pop() {
        if original.contains(&id) {
            continue;
        }
        let snapshot = sorrel_core::read_snapshot(&store, &id).map_err(store_error)?;
        if !expanded {
            if !visited.insert(id) {
                continue;
            }
            queue.push((id, true));
            for parent in snapshot.parents.iter().rev() {
                queue.push((parent.id, false));
            }
            continue;
        }
        let change = history
            .get(&id)
            .map(|change| sorrel_core::read_change(&store, change).map_err(store_error))
            .transpose()?;
        let author = change
            .as_ref()
            .map(|change| &change.author)
            .unwrap_or(&snapshot.author);
        let message = change
            .as_ref()
            .map(|change| change.message.clone())
            .or(snapshot.message);
        commits.push(json!({
            "snapshot":{"kind":"Snapshot","id":id.to_hex()},
            "change":change.as_ref().map(|change| json!({"kind":"Change","id":change.id.to_hex()})),
            "author":{"type":author.principal_type,"id":author.id,"displayName":author.display_name},
            "message":message,"createdAt":snapshot.created_at,
            "pending":!owner.ancestors.contains(&id),
        }));
    }
    Ok(Review {
        store,
        base,
        tip,
        workspace,
        overview,
        commits,
        reachable,
        history,
        change_objects,
        _worker_lock: worker_lock,
    })
}

pub fn review(id: &str) -> io::Result<Review> {
    let owner = owner_state()?;
    inspect(read_json(&record_path(id)?)?, &owner)
}

/// Keep unavailable or malformed worker rows visible without hiding other work.
/// Errors in owner metadata still fail the entire view.
pub fn overview() -> io::Result<Vec<Value>> {
    let owner = owner_state()?;
    let workspaces = repo::list_registry_entries("workspaces")?;
    let mut rows = Vec::new();
    for workspace in workspaces {
        validate_workspace(&workspace)?;
        match inspect(workspace.clone(), &owner) {
            Ok(review) => rows.push(review.overview),
            Err(error) => {
                let mut row = workspace;
                row["status"] = json!(if error.kind() == io::ErrorKind::NotFound {
                    "missing"
                } else {
                    "invalid"
                });
                row["readyToIntegrate"] = json!(false);
                row["error"] =
                    json!({"code":crate::error_code(&error),"message":error.to_string()});
                rows.push(row);
            }
        }
    }
    Ok(rows)
}

pub fn list() -> io::Result<CommandOutput> {
    let workspaces = overview()?;
    let human = workspaces
        .iter()
        .map(|workspace| {
            format!(
                "{}  {}  pending={}  {}{}",
                workspace["id"].as_str().unwrap_or("?"),
                workspace["status"].as_str().unwrap_or("?"),
                workspace["pendingSnapshots"]
                    .as_u64()
                    .map_or_else(|| "?".to_owned(), |n| n.to_string()),
                workspace["path"].as_str().unwrap_or("?"),
                workspace["task"]
                    .as_str()
                    .map_or_else(String::new, |task| format!("  {task}")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(CommandOutput {
        json: json!({"command":"workspace list","workspaces":workspaces}),
        human,
    })
}

pub struct Integration {
    pub lane: String,
    pub workspace: Value,
    _worker_lock: repo::RepositoryLock,
}

pub fn prepare_integration(id: &str, expected: Option<&str>) -> io::Result<Integration> {
    let review = review(id).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            command_error("invalid_workspace", error.to_string())
        } else {
            error
        }
    })?;
    if let Some(expected) = expected {
        let expected: ObjectId = expected.parse().map_err(|_| {
            command_error(
                "invalid_input",
                "reviewed snapshot must be a valid object id",
            )
        })?;
        if expected != review.tip {
            return Err(command_error("review_stale", "worker recorded new work since review; review the current snapshot before integrating"));
        }
    }
    if review.overview["recovering"] == true {
        return Err(command_error(
            "recovery_required",
            "finish or recover the agent workspace before integration",
        ));
    }
    if review.overview["activeLane"] != review.workspace["lane"] {
        return Err(command_error(
            "invalid_workspace",
            "agent workspace changed its assigned lane",
        ));
    }
    if review.overview["dirty"] == true {
        return Err(command_error(
            "dirty_worktree",
            "agent workspace has unrecorded edits; record them before integration",
        ));
    }
    let mut owner_history = history_index(&repo::changes_index_path())?;
    for (result, change) in &review.history {
        if review.reachable.contains(result) {
            owner_history.insert(*result, *change);
        }
    }
    let store = FileObjectStore::new(repo::object_store_root()).map_err(store_error)?;
    for id in sorrel_core::collect_closure(&review.store, &[review.tip])
        .map_err(store_error)?
        .into_iter()
        .chain(review.change_objects)
    {
        store
            .write(&review.store.read(&id).map_err(store_error)?)
            .map_err(store_error)?;
    }
    repo::write_bytes_atomic(&repo::changes_index_path(), &index_bytes(&owner_history)?)?;
    let lane = review.workspace["lane"]
        .as_str()
        .ok_or_else(|| command_error("invalid_data", "workspace missing lane"))?
        .to_owned();
    repo::write_lane_head(&lane, &review.tip.to_hex())?;
    Ok(Integration {
        lane,
        workspace: review.workspace,
        _worker_lock: review._worker_lock,
    })
}
