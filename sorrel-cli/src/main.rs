use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use cli_policy::{
    evaluate, evaluate_policy_change, Decision, EvaluateInput, PolicyChange, PolicyContext,
    PrincipalId, ResourceRef,
};
use serde_json::{json, Value};
use sorrel_core::merge3::{merge3, MergeOutcome};
use sorrel_core::{
    create_change, create_lane, create_stack, git_export, git_import, is_descendant,
    materialize_snapshot_excluding_with_stat_cache, merge_base, merge_snapshots,
    parse_object_id_hex, read_conflict, read_snapshot, read_snapshot_files, read_stack,
    restore_snapshot_to_directory, snapshot_diff, write_snapshot, write_tree, ChangeOptions,
    ConflictType, FileObjectStore, GitExportOptions, GitImportOptions, ImportResult,
    ImportedCommit, LaneOptions, MergeOptions, ObjectId, ObjectKind, ObjectRef, ObjectStore,
    PathChangeKind, Principal, SnapshotOptions, StackOptions, StatCache, Visibility,
};

use sorrel_cli::{cli_policy, hub, linediff, repo, sync, CommandOutput};

use sorrel_cli::workflow_cmd::{self, WorkflowFileArgs, WorkflowRunJobArgs};

const PROTOCOL_VERSION: &str = "sorrel.protocol.v0";
/// Fixed evaluation timestamp for policy decision/evaluation output so that
/// `--json` shapes stay deterministic for tooling and tests. (Policy evaluation
/// is pure given its inputs; the wall-clock stamp is not part of the decision.)
const POLICY_EVALUATED_AT: &str = "2026-06-24T09:00:00Z";

#[derive(Debug, Parser)]
#[command(name = "sorrel")]
#[command(about = "Sorrel agent-native version control CLI")]
#[command(version)]
struct Cli {
    /// Emit stable structured JSON output.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize Sorrel metadata for the current repository.
    Init,
    /// Show Sorrel repository status (real dirty detection vs HEAD).
    Status,
    /// Show line-level differences between the working tree and HEAD.
    Diff(DiffArgs),
    /// Show the change history reachable from HEAD.
    Log(LogArgs),
    /// Work with Sorrel Change objects.
    Change {
        #[command(subcommand)]
        command: ChangeCommand,
    },
    /// Work with Sorrel Lane objects.
    Lane {
        #[command(subcommand)]
        command: LaneCommand,
    },
    /// Work with Sorrel Stack objects.
    Stack {
        #[command(subcommand)]
        command: StackCommand,
    },
    /// Work with Sorrel Slice objects.
    Slice {
        #[command(subcommand)]
        command: SliceCommand,
    },
    /// Work with Sorrel Workflow objects.
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    /// Inspect and exercise Sorrel policy decisions.
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },
    /// Create and list Sorrel permission grants.
    Grant {
        #[command(subcommand)]
        command: GrantCommand,
    },
    /// Inspect and manage Sorrel secret handles via SecretSpec providers.
    Secret {
        #[command(subcommand)]
        command: sorrel_cli::secret_cmd::SecretCommand,
    },
    /// Detect and scaffold project environments (devenv-first).
    Env {
        #[command(subcommand)]
        command: sorrel_cli::env_cmd::EnvCommand,
    },
    /// Inspect local workflow/task execution logs.
    Run {
        #[command(subcommand)]
        command: sorrel_cli::run_log::RunCommand,
    },
    /// Configure and inspect sync remotes.
    Remote {
        #[command(subcommand)]
        command: RemoteCommand,
    },
    /// Push local HEAD to a configured remote.
    Push(PushArgs),
    /// Pull a remote ref and update local HEAD.
    Pull(PullArgs),
    /// Merge another lane into the active lane.
    Merge(MergeArgs),
    /// Git bridge commands (import, export, colocated sync).
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
}

#[derive(Debug, Args)]
struct MergeArgs {
    /// Lane id to merge into the active lane.
    lane_id: Option<String>,

    /// Abort an in-progress conflicted merge and restore the pre-merge tree.
    #[arg(long)]
    abort: bool,

    /// Finalize an in-progress merge after conflicts are resolved in the worktree.
    #[arg(long = "continue")]
    r#continue: bool,
}

#[derive(Debug, Subcommand)]
enum GitCommand {
    /// Import Git history into this Sorrel workspace (one-way).
    Import(GitImportArgs),
    /// Export Sorrel snapshot history into a Git repository (one-way).
    Export(GitExportArgs),
    /// Keep a colocated Git mirror in sync (bidirectional fast-forward).
    Sync(GitSyncArgs),
}

#[derive(Debug, Args)]
struct GitSyncArgs {
    /// Path to the mirrored Git repository. Defaults to `.` (colocated).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Git branch to keep in sync.
    #[arg(long, default_value = "main")]
    branch: String,

    /// Restore the working tree even when it has uncommitted changes.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct GitExportArgs {
    /// Path for the destination Git repository. Defaults to `.`.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Branch to update (created if missing).
    #[arg(long, default_value = "main")]
    branch: String,

    /// Snapshot id to export (defaults to HEAD).
    #[arg(long)]
    snapshot: Option<String>,

    /// Overwrite / proceed even when the destination already has commits on the branch.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Subcommand)]
enum StackCommand {
    /// Create a Stack from the current HEAD (single tip change span).
    Create(StackCreateArgs),
    /// List registered stacks.
    List,
    /// Show a stack by id.
    Show(StackShowArgs),
}

#[derive(Debug, Args)]
struct StackCreateArgs {
    /// Name for the new Stack.
    #[arg(long, default_value = "stack/default")]
    name: String,

    /// Optional description.
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
struct StackShowArgs {
    /// Stack id (from `sorrel stack list`).
    stack_id: String,
}

#[derive(Debug, Args)]
struct GitImportArgs {
    /// Path to the Git repository (working tree or bare). Defaults to `.`.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Git ref to import (default `HEAD`).
    #[arg(long = "ref", default_value = "HEAD")]
    git_ref: String,

    /// Import at most N newest commits reachable from the ref.
    #[arg(long)]
    limit: Option<usize>,

    /// Proceed even if the working tree is dirty or a previous git-map exists.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Subcommand)]
enum ChangeCommand {
    /// Create a Change from the working tree.
    Create(ChangeCreateArgs),
    /// List recorded Change objects.
    List,
}

#[derive(Debug, Args)]
struct ChangeCreateArgs {
    /// Short message describing the change.
    #[arg(short = 'm', long)]
    message: String,

    /// Optional longer description.
    #[arg(long)]
    description: Option<String>,
}

#[derive(Debug, Args)]
struct DiffArgs {
    /// Maximum number of context lines is not configurable yet; reserved.
    #[arg(long, hide = true, default_value_t = 3)]
    context: usize,
}

#[derive(Debug, Args)]
struct LogArgs {
    /// Limit the number of changes shown (most recent first).
    #[arg(long)]
    limit: Option<usize>,
}

#[derive(Debug, Subcommand)]
enum LaneCommand {
    /// Create a Lane from the current HEAD.
    Create(LaneCreateArgs),
    /// List registered lanes and their head snapshots.
    List,
    /// Switch the active lane and restore its head snapshot.
    Switch(LaneSwitchArgs),
    /// Submit the active lane tip to Hub as a collaboration proposal.
    Submit(LaneSubmitArgs),
}

#[derive(Debug, Args)]
struct LaneCreateArgs {
    /// Name for the new Lane.
    #[arg(long, default_value = "agent/mock-lane")]
    name: String,
}

#[derive(Debug, Args)]
struct LaneSwitchArgs {
    /// Lane id to activate (from `sorrel lane list`).
    lane_id: String,
}

#[derive(Debug, Args)]
struct LaneSubmitArgs {
    /// Remote name configured via `sorrel remote add` (Hub base URL).
    #[arg(long, default_value = "origin")]
    remote: String,

    /// Hub project id. When omitted, a project named after the sync repo id is ensured.
    #[arg(long)]
    project_id: Option<String>,

    /// Organization id used when auto-creating a Hub project.
    #[arg(long, default_value = "org_local")]
    organization_id: String,

    /// Proposal title (defaults to "Submit <lane>").
    #[arg(long)]
    title: Option<String>,

    /// Target lane id recorded on the proposal.
    #[arg(long, default_value = "lane_main")]
    target_lane: String,

    /// Push the active lane tip to the remote before submitting.
    #[arg(long, default_value_t = true)]
    push: bool,

    /// Skip pushing even if the remote is configured.
    #[arg(long)]
    no_push: bool,
}

#[derive(Debug, Subcommand)]
enum SliceCommand {
    /// Create a Slice manifest.
    Create(SliceCreateArgs),
}

#[derive(Debug, Args)]
struct SliceCreateArgs {
    /// Name for the new Slice.
    #[arg(long, default_value = "mock-slice")]
    name: String,

    /// Source repository for the Slice.
    #[arg(long, default_value = "sorrel://local/repo")]
    source_repo: String,

    /// Source path for the Slice.
    #[arg(long, default_value = "src")]
    source_path: String,

    /// Entrypoint path for the Slice.
    #[arg(long, default_value = "src/main.rs")]
    entrypoint: String,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    /// Validate a sorrel.workflow.yml file.
    Validate(WorkflowFileArgs),
    /// Run a named job from a sorrel.workflow.yml file.
    Run(WorkflowRunJobArgs),
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    /// Evaluate a Sorrel Core permission decision.
    Evaluate(PolicyEvaluateArgs),
    /// Evaluate signed policy changes against the previous effective policy.
    Change {
        #[command(subcommand)]
        command: PolicyChangeCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PolicyChangeCommand {
    /// Evaluate a PolicyChange before it would be applied.
    Apply(PolicyChangeApplyArgs),
}

#[derive(Debug, Args)]
struct PolicyEvaluateArgs {
    /// Principal requesting permission, such as agent:docs.
    #[arg(long, default_value = "agent:agent_mock_cli")]
    principal: String,

    /// Core permission action to evaluate, such as path.write.
    #[arg(long, default_value = "path.write")]
    action: String,

    /// Resource reference, such as path:README.md or secret:secret_database_url_dev.
    #[arg(long, default_value = "path:README.md")]
    resource: String,

    /// Environment for secret.* permissions.
    #[arg(long, default_value = "dev")]
    environment: String,
}

#[derive(Debug, Args)]
struct PolicyChangeApplyArgs {
    /// JSON PolicyChange document to evaluate.
    #[arg(long)]
    file: Option<String>,

    /// Actor principal for inline policy changes.
    #[arg(long)]
    actor: Option<String>,

    /// Policy change operation, such as grant.
    #[arg(long, default_value = "grant")]
    operation: String,

    /// Target principal for grant operations.
    #[arg(long)]
    target_principal: Option<String>,

    /// Capability to grant. Repeat for multiple capabilities.
    #[arg(long = "capability")]
    capabilities: Vec<String>,

    /// Signature attached to the policy change. Repeat for multiple signatures.
    #[arg(long = "signature")]
    signatures: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum GrantCommand {
    /// Create a permission grant and evaluate it via Core.
    Create(GrantCreateArgs),
    /// List persisted permission grants.
    List,
}

#[derive(Debug, Args)]
struct GrantCreateArgs {
    /// Core permission action the grant authorizes.
    #[arg(long, default_value = "secret.inject")]
    action: String,

    /// Agent policy allowed by the grant.
    #[arg(long, default_value = "agent_mock_cli")]
    agent: String,

    /// Workflow allowed by the grant.
    #[arg(long, default_value = "workflow_validate_vault")]
    workflow: String,

    /// Runner allowed by the grant.
    #[arg(long, default_value = "runner_local_process")]
    runner: String,

    /// SecretRef id for secret.* grants.
    #[arg(long, default_value = "secret_database_url_dev")]
    secret: String,

    /// Environment for secret.* grants.
    #[arg(long, default_value = "dev")]
    environment: String,

    /// Human-readable reason attached to the grant.
    #[arg(
        long,
        default_value = "Local validation can inject the dev secret handle."
    )]
    reason: String,
}

#[derive(Debug, Subcommand)]
enum RemoteCommand {
    /// Add or update a named remote.
    Add(RemoteAddArgs),
    /// List configured remotes.
    List,
}

#[derive(Debug, Args)]
struct RemoteAddArgs {
    /// Remote name (e.g. origin).
    name: String,

    /// Sync transport base URL.
    url: String,

    /// Remote repository id (defaults to local manifest repoId).
    #[arg(long)]
    repo_id: Option<String>,
}

#[derive(Debug, Args)]
struct PushArgs {
    /// Remote name (default: origin).
    remote: Option<String>,

    /// Ref to push (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    r#ref: String,
}

#[derive(Debug, Args)]
struct PullArgs {
    /// Remote name (default: origin).
    remote: Option<String>,

    /// Ref to pull (default: HEAD).
    #[arg(long, default_value = "HEAD")]
    r#ref: String,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sorrel: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> io::Result<()> {
    if let Commands::Secret {
        command: sorrel_cli::secret_cmd::SecretCommand::Run(args),
    } = cli.command
    {
        return match sorrel_cli::secret_cmd::execute_run(args, cli.json)? {
            sorrel_cli::secret_cmd::SecretRunResult::Output(output) => {
                if cli.json {
                    write_json(io::stdout().lock(), &output.json)
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", output.human)
                }
            }
            sorrel_cli::secret_cmd::SecretRunResult::Exit(code) => {
                std::process::exit(code);
            }
        };
    }

    let output = execute(cli.command)?;

    if cli.json {
        write_json(io::stdout().lock(), &output.json)
    } else {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", output.human)
    }
}

fn execute(command: Commands) -> io::Result<CommandOutput> {
    match command {
        Commands::Init => init_output(),
        Commands::Status => status_output(),
        Commands::Diff(args) => diff_output(args),
        Commands::Log(args) => log_output(args),
        Commands::Change { command } => match command {
            ChangeCommand::Create(args) => change_create_output(args),
            ChangeCommand::List => change_list_output(),
        },
        Commands::Lane { command } => match command {
            LaneCommand::Create(args) => lane_create_output(args),
            LaneCommand::List => lane_list_output(),
            LaneCommand::Switch(args) => lane_switch_output(args),
            LaneCommand::Submit(args) => lane_submit_output(args),
        },
        Commands::Stack { command } => match command {
            StackCommand::Create(args) => stack_create_output(args),
            StackCommand::List => stack_list_output(),
            StackCommand::Show(args) => stack_show_output(args),
        },
        Commands::Slice { command } => match command {
            SliceCommand::Create(args) => slice_create_output(args),
        },
        Commands::Workflow { command } => match command {
            WorkflowCommand::Validate(args) => Ok(workflow_cmd::workflow_validate_output(args)),
            WorkflowCommand::Run(args) => Ok(workflow_cmd::workflow_run_output(args)),
        },
        Commands::Policy { command } => match command {
            PolicyCommand::Evaluate(args) => policy_evaluate_output(args),
            PolicyCommand::Change { command } => match command {
                PolicyChangeCommand::Apply(args) => policy_change_apply_output(args),
            },
        },
        Commands::Grant { command } => match command {
            GrantCommand::Create(args) => grant_create_output(args),
            GrantCommand::List => grant_list_output(),
        },
        Commands::Secret { command } => sorrel_cli::secret_cmd::execute(command),
        Commands::Env { command } => sorrel_cli::env_cmd::execute(command),
        Commands::Run { command } => sorrel_cli::run_log::execute(command),
        Commands::Remote { command } => match command {
            RemoteCommand::Add(args) => remote_add_output(args),
            RemoteCommand::List => remote_list_output(),
        },
        Commands::Push(args) => push_output(args),
        Commands::Pull(args) => pull_output(args),
        Commands::Merge(args) => merge_output(args),
        Commands::Git { command } => match command {
            GitCommand::Import(args) => git_import_output(args),
            GitCommand::Export(args) => git_export_output(args),
            GitCommand::Sync(args) => git_sync_output(args),
        },
    }
}

fn init_output() -> io::Result<CommandOutput> {
    // Idempotent-safe: never clobber an existing repository's history.
    if repo::is_initialized() {
        let manifest = repo::load_manifest()?.unwrap_or_else(|| json!({}));
        let repo_id = manifest
            .get("repoId")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        return Ok(CommandOutput {
            json: json!({
                "command": "init",
                "mocked": false,
                "repoId": repo_id,
                "sorrelDir": repo::SORREL_DIR,
                "initialized": true,
                "status": "already_initialized"
            }),
            human: format!("Sorrel repository {repo_id} already initialized in .sorrel"),
        });
    }

    fs::create_dir_all(repo::sorrel_dir().join(repo::SLICES_DIR))?;
    fs::create_dir_all(repo::registry_dir(repo::LANES_DIR))?;
    fs::create_dir_all(repo::registry_dir(repo::STACKS_DIR))?;
    fs::create_dir_all(repo::heads_dir())?;

    let store = to_io(FileObjectStore::new(repo::object_store_root()))?;

    // The initial HEAD is an empty (unborn) tree. Build it directly from an
    // empty tree object instead of materializing a directory, so init touches
    // no temp scratch and never risks recursing into `.sorrel/`.
    let repo_id = repo::generate_repo_id();
    let created_at = repo::now_rfc3339();
    let mut options = SnapshotOptions::new(repo_id.clone());
    options.created_at = created_at.clone();
    options.message = Some("initial snapshot".to_owned());
    let empty_tree = to_io(write_tree(&store, Vec::new()))?;
    let snapshot = to_io(write_snapshot(&store, empty_tree.id, options))?;
    let head_snapshot = snapshot.id.to_hex();

    let manifest = repo::build_manifest(&repo_id, &created_at);
    repo::write_manifest(&manifest)?;
    // write_head also seeds `.sorrel/heads/<default-lane>`.
    repo::write_head(&repo::Head {
        lane: repo::DEFAULT_LANE_ID.to_owned(),
        snapshot: head_snapshot.clone(),
    })?;
    // Register the default lane so `lane list` has an entry after init.
    let default_lane = json!({
        "kind": "Lane",
        "id": repo::DEFAULT_LANE_ID,
        "name": repo::DEFAULT_LANE_NAME,
        "baseSnapshot": { "kind": "Snapshot", "id": head_snapshot },
        "headSnapshot": { "kind": "Snapshot", "id": head_snapshot },
        "createdAt": created_at,
    });
    repo::write_registry_entry(repo::LANES_DIR, repo::DEFAULT_LANE_ID, &default_lane)?;

    Ok(CommandOutput {
        json: json!({
            "command": "init",
            "mocked": false,
            "repoId": repo_id,
            "sorrelDir": repo::SORREL_DIR,
            "initialized": true,
            "status": "initialized",
            "createdAt": created_at,
            "defaultLane": { "id": repo::DEFAULT_LANE_ID, "name": repo::DEFAULT_LANE_NAME },
            "headSnapshot": { "kind": "Snapshot", "id": head_snapshot }
        }),
        human: format!("Initialized Sorrel repository {repo_id} in .sorrel"),
    })
}

fn status_output() -> io::Result<CommandOutput> {
    let Some(manifest) = repo::load_manifest()? else {
        return Ok(CommandOutput {
            json: json!({
                "command": "status",
                "mocked": false,
                "sorrelDir": repo::SORREL_DIR,
                "initialized": false,
                "status": "uninitialized",
                "currentLane": null,
                "headSnapshot": null
            }),
            human: "Sorrel workspace is not initialized; run `sorrel init`".to_owned(),
        });
    };

    let repo_id = manifest
        .get("repoId")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let head = repo::load_head()?;
    let lane = head
        .as_ref()
        .map(|head| head.lane.clone())
        .unwrap_or_else(|| repo::DEFAULT_LANE_ID.to_owned());
    let head_snapshot = head
        .as_ref()
        .map(|head| head.snapshot.clone())
        .filter(|snapshot| !snapshot.is_empty());

    // Real working-tree dirty detection: snapshot the current tree (minus
    // `.sorrel/`) and diff it against HEAD.
    let store = to_io(FileObjectStore::new(repo::object_store_root()))?;
    let (worktree_json, dirty, status_label) = match head.as_ref().and_then(|head| {
        head_snapshot_id(head)
            .transpose()
            .map(|id| id.map(|id| (head, id)))
    }) {
        Some(result) => {
            let (_, base_id) = result?;
            let mut stat_cache = repo::load_stat_cache();
            let current = materialize_worktree(&store, &repo_id, None, &[], Some(&mut stat_cache))?;
            repo::save_stat_cache(&stat_cache)?;
            let diff = to_io(snapshot_diff(&store, &base_id, &current))?;
            let (changes, total) = diff_json(&diff);
            let dirty = total > 0;
            (
                json!({ "dirty": dirty, "changes": changes, "conflicts": 0 }),
                dirty,
                if dirty { "dirty" } else { "clean" },
            )
        }
        None => (
            json!({ "dirty": false, "changes": json!({"added": [], "modified": [], "deleted": []}), "conflicts": 0 }),
            false,
            "clean",
        ),
    };

    let human = if dirty {
        format!("Sorrel repository {repo_id} on lane {lane}: dirty")
    } else {
        format!("Sorrel repository {repo_id} on lane {lane}: clean")
    };

    Ok(CommandOutput {
        json: json!({
            "command": "status",
            "mocked": false,
            "repoId": repo_id,
            "sorrelDir": repo::SORREL_DIR,
            "initialized": true,
            "status": status_label,
            "currentLane": { "kind": "Lane", "id": lane },
            "headSnapshot": head_snapshot
                .as_ref()
                .map(|id| json!({ "kind": "Snapshot", "id": id }))
                .unwrap_or(Value::Null),
            "worktree": worktree_json
        }),
        human,
    })
}

fn change_create_output(args: ChangeCreateArgs) -> io::Result<CommandOutput> {
    let Some(manifest) = repo::load_manifest()? else {
        return Err(io::Error::other(
            "workspace is not initialized; run `sorrel init`",
        ));
    };
    let repo_id = manifest
        .get("repoId")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();

    let head = repo::load_head()?.ok_or_else(|| io::Error::other("missing HEAD pointer"))?;
    let base_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    let store = to_io(FileObjectStore::new(repo::object_store_root()))?;

    // Snapshot the working tree (minus `.sorrel/`) and diff it against HEAD.
    let mut stat_cache = repo::load_stat_cache();
    let new_snapshot = materialize_worktree(
        &store,
        &repo_id,
        Some(args.message.clone()),
        &[base_id],
        Some(&mut stat_cache),
    )?;
    repo::save_stat_cache(&stat_cache)?;
    let diff = to_io(snapshot_diff(&store, &base_id, &new_snapshot))?;

    if diff.is_empty() {
        return Err(io::Error::other("no changes to record since HEAD"));
    }

    let change = to_io(create_change(&store, base_id, new_snapshot, {
        let mut options = ChangeOptions::new(Principal::system(), args.message.clone());
        options.description = args.description.clone();
        options
    }))?;

    // Advance HEAD to the new snapshot on the current lane.
    repo::write_head(&repo::Head {
        lane: head.lane.clone(),
        snapshot: new_snapshot.to_hex(),
    })?;

    // Record snapshot → change so `log` can resolve Change metadata later.
    let change_id = change.id.to_hex();
    repo::append_changes_index(&repo::ChangesIndexEntry {
        snapshot: new_snapshot.to_hex(),
        change: change_id.clone(),
    })?;

    let (changes, total) = diff_json(&change.diff);

    Ok(CommandOutput {
        json: json!({
            "command": "change create",
            "mocked": false,
            "status": "created",
            "object": {
                "kind": "Change",
                "id": change_id,
                "message": args.message,
                "description": args.description,
                "baseSnapshot": { "kind": "Snapshot", "id": base_id.to_hex() },
                "resultingSnapshot": { "kind": "Snapshot", "id": new_snapshot.to_hex() },
                "changedPaths": total,
                "diff": changes
            }
        }),
        human: format!("Created change {change_id} ({total} path(s))"),
    })
}

/// Loaded repository context shared by read-only commands.
struct RepoContext {
    repo_id: String,
    head: repo::Head,
    store: FileObjectStore,
}

/// Loads manifest + HEAD + object store, or errors if uninitialized.
fn open_repo() -> io::Result<RepoContext> {
    let Some(manifest) = repo::load_manifest()? else {
        return Err(io::Error::other(
            "workspace is not initialized; run `sorrel init`",
        ));
    };
    let repo_id = manifest
        .get("repoId")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let head = repo::load_head()?.ok_or_else(|| io::Error::other("missing HEAD pointer"))?;
    let store = to_io(FileObjectStore::new(repo::object_store_root()))?;
    Ok(RepoContext {
        repo_id,
        head,
        store,
    })
}

fn diff_output(_args: DiffArgs) -> io::Result<CommandOutput> {
    let context = 3usize;
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    let base_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    // Snapshot the current working tree (minus `.sorrel/`) and diff vs HEAD.
    // Diff is a read-only view and does not persist the stat cache.
    let current = materialize_worktree(&store, &repo_id, None, &[], None)?;
    let diff = to_io(snapshot_diff(&store, &base_id, &current))?;

    let base_files = to_io(read_snapshot_files(&store, &base_id))?;
    let current_files = to_io(read_snapshot_files(&store, &current))?;

    let mut files_json = Vec::new();
    let mut human = String::new();
    for change in &diff.changes {
        let path = change.path.clone();
        let path_str = path.to_string_lossy().into_owned();
        let kind = match change.kind {
            PathChangeKind::Added => "added",
            PathChangeKind::Modified => "modified",
            PathChangeKind::Deleted => "deleted",
        };

        let old_bytes = base_files.get(&path).map(Vec::as_slice).unwrap_or(&[]);
        let new_bytes = current_files.get(&path).map(Vec::as_slice).unwrap_or(&[]);

        let (file_json, file_human) = match (
            std::str::from_utf8(old_bytes),
            std::str::from_utf8(new_bytes),
        ) {
            (Ok(old_text), Ok(new_text)) => {
                let hunks = linediff::hunks(old_text, new_text, context);
                let rendered = linediff::render_unified(&hunks);
                let hunks_json: Vec<Value> = hunks
                    .iter()
                    .map(|hunk| {
                        json!({
                            "oldStart": hunk.old_start,
                            "oldLen": hunk.old_len,
                            "newStart": hunk.new_start,
                            "newLen": hunk.new_len,
                            "lines": hunk.lines.iter().map(|line| json!({
                                "kind": match line.kind {
                                    linediff::LineKind::Context => "context",
                                    linediff::LineKind::Added => "added",
                                    linediff::LineKind::Removed => "removed",
                                },
                                "text": line.text,
                            })).collect::<Vec<_>>()
                        })
                    })
                    .collect();
                (
                    json!({ "path": path_str, "kind": kind, "binary": false, "hunks": hunks_json }),
                    format!("diff --sorrel {path_str} ({kind})\n{rendered}"),
                )
            }
            _ => (
                json!({ "path": path_str, "kind": kind, "binary": true, "hunks": [] }),
                format!("diff --sorrel {path_str} ({kind})\nBinary file changed\n"),
            ),
        };

        files_json.push(file_json);
        human.push_str(&file_human);
    }

    if files_json.is_empty() {
        human = "No changes against HEAD".to_owned();
    }

    Ok(CommandOutput {
        json: json!({
            "command": "diff",
            "mocked": false,
            "repoId": repo_id,
            "baseSnapshot": { "kind": "Snapshot", "id": base_id.to_hex() },
            "files": files_json
        }),
        human: human.trim_end().to_owned(),
    })
}

fn log_output(args: LogArgs) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;

    // Walk snapshot ancestry from HEAD. Each `change create` materializes a
    // resulting snapshot whose first parent is the prior HEAD snapshot, so the
    // first-parent chain reconstructs history back to the initial snapshot.
    let head_snapshot =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    // Optional index: missing/corrupt entries never break log (pre-index repos,
    // initial snapshot, etc.).
    let changes_index = repo::load_changes_index();

    let mut entries: Vec<Value> = Vec::new();
    let mut human = String::new();
    let mut current = Some(head_snapshot);
    let mut seen = std::collections::BTreeSet::new();

    while let Some(snapshot_id) = current {
        if !seen.insert(snapshot_id.to_hex()) {
            break;
        }
        let snapshot = to_io(read_snapshot(&store, &snapshot_id))?;
        // The initial snapshot has no parents and no message we treat as a change.
        let parent = snapshot.parents.first().map(|parent| parent.id);

        if let Some(snapshot_message) = snapshot.message.clone() {
            let is_initial = parent.is_none();
            let snapshot_hex = snapshot_id.to_hex();

            // Prefer Change metadata from the index when available; fall back to
            // the snapshot message/timestamp (e.g. initial snapshot, pre-index).
            // Load via the object store + JSON so a missing typed reader never
            // breaks log; failed lookups degrade to the snapshot-only shape.
            let entry = match load_indexed_change(&store, &changes_index, &snapshot_hex) {
                Some(change) => json!({
                    "snapshot": { "kind": "Snapshot", "id": snapshot_hex },
                    "change": { "kind": "Change", "id": change.id },
                    "author": change.author,
                    "message": change.message,
                    "createdAt": change
                        .created_at
                        .unwrap_or_else(|| snapshot.created_at.clone()),
                    "root": is_initial,
                }),
                None => json!({
                    "snapshot": { "kind": "Snapshot", "id": snapshot_hex },
                    "change": Value::Null,
                    "author": Value::Null,
                    "message": snapshot_message,
                    "createdAt": snapshot.created_at,
                    "root": is_initial,
                }),
            };
            entries.push(entry);
        }

        if let Some(limit) = args.limit {
            if entries.len() >= limit {
                break;
            }
        }
        current = parent;
    }

    for entry in &entries {
        human.push_str(&format_log_entry_human(entry));
        human.push('\n');
    }
    if entries.is_empty() {
        human = "No history yet".to_owned();
    }

    Ok(CommandOutput {
        json: json!({
            "command": "log",
            "mocked": false,
            "repoId": repo_id,
            "count": entries.len(),
            "entries": entries
        }),
        human: human.trim_end().to_owned(),
    })
}

/// Formats one `log` entry for human output.
///
/// Indexed changes: short change id, author, timestamp, message, short snapshot.
/// Unindexed snapshots (e.g. initial): short snapshot id and message, as before.
fn format_log_entry_human(entry: &Value) -> String {
    let snapshot_id = entry["snapshot"]["id"].as_str().unwrap_or_default();
    let message = entry["message"].as_str().unwrap_or_default();
    let short_snapshot = &snapshot_id[..snapshot_id.len().min(12)];

    if let Some(change_id) = entry["change"]["id"].as_str() {
        let author = entry["author"].as_str().unwrap_or_default();
        let created_at = entry["createdAt"].as_str().unwrap_or_default();
        let short_change = &change_id[..change_id.len().min(12)];
        format!("{short_change}  {author}  {created_at}  {message}  ({short_snapshot})")
    } else {
        format!("{short_snapshot}  {message}")
    }
}

/// Change metadata resolved through `.sorrel/changes.index` + the object store.
struct IndexedChange {
    id: String,
    author: String,
    message: String,
    /// Change objects don't carry a timestamp in the engine schema; log falls
    /// back to the snapshot's `createdAt` when this is absent.
    created_at: Option<String>,
}

/// Looks up `snapshot_hex` in the index and loads Change fields from the store.
///
/// Returns `None` when the index has no entry, the change id is invalid, the
/// object is missing/unreadable, or required fields are absent — never errors.
fn load_indexed_change(
    store: &FileObjectStore,
    index: &std::collections::BTreeMap<String, String>,
    snapshot_hex: &str,
) -> Option<IndexedChange> {
    let change_hex = index.get(snapshot_hex)?;
    let change_id = change_hex.parse::<ObjectId>().ok()?;
    let bytes = store.read(&change_id).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let message = json_field_str(&value, "message", "message")?.to_owned();
    let created_at = json_field_str(&value, "created_at", "createdAt").map(str::to_owned);
    let author = author_from_change_json(&value)?;
    Some(IndexedChange {
        id: change_hex.clone(),
        author,
        message,
        created_at,
    })
}

fn json_field_str<'a>(value: &'a Value, snake: &str, camel: &str) -> Option<&'a str> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(Value::as_str)
}

fn author_from_change_json(value: &Value) -> Option<String> {
    match value.get("author") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Object(map)) => {
            let kind = map
                .get("kind")
                .or_else(|| map.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let id = map.get("id").and_then(Value::as_str).unwrap_or_default();
            Some(format!("{kind}:{id}"))
        }
        _ => None,
    }
}

fn change_list_output() -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;

    // Walk the snapshot DAG from HEAD; each non-root snapshot corresponds to a
    // recorded change (same traversal as `log`, surfaced as change objects).
    let head_snapshot =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    let mut objects: Vec<Value> = Vec::new();
    let mut human = String::new();
    let mut current = Some(head_snapshot);
    let mut seen = std::collections::BTreeSet::new();

    while let Some(snapshot_id) = current {
        if !seen.insert(snapshot_id.to_hex()) {
            break;
        }
        let snapshot = to_io(read_snapshot(&store, &snapshot_id))?;
        let parent = snapshot.parents.first().map(|parent| parent.id);

        // The initial (parentless) snapshot is not a change.
        if let (Some(message), Some(base)) = (snapshot.message.clone(), parent) {
            objects.push(json!({
                "kind": "Change",
                "message": message,
                "createdAt": snapshot.created_at,
                "baseSnapshot": { "kind": "Snapshot", "id": base.to_hex() },
                "resultingSnapshot": { "kind": "Snapshot", "id": snapshot_id.to_hex() },
            }));
        }
        current = parent;
    }

    for object in &objects {
        let id = object["resultingSnapshot"]["id"]
            .as_str()
            .unwrap_or_default();
        let message = object["message"].as_str().unwrap_or_default();
        human.push_str(&format!("{}  {message}\n", &id[..id.len().min(12)]));
    }
    if objects.is_empty() {
        human = "No changes recorded yet".to_owned();
    }

    Ok(CommandOutput {
        json: json!({
            "command": "change list",
            "mocked": false,
            "repoId": repo_id,
            "count": objects.len(),
            "objects": objects
        }),
        human: human.trim_end().to_owned(),
    })
}

fn lane_create_output(args: LaneCreateArgs) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    let base_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    // A new lane branches from the current HEAD snapshot (base == head == HEAD).
    let mut options = LaneOptions::new(
        args.name.clone(),
        base_id,
        base_id,
        Principal::system(),
        Visibility::Private,
    );
    options.created_at = repo::now_rfc3339();
    let lane = to_io(create_lane(&store, options))?;
    let lane_id = lane.id.to_hex();
    let head_snapshot = base_id.to_hex();

    // Persist a lane registry entry: name -> lane object id + per-lane HEAD.
    let entry = json!({
        "kind": "Lane",
        "id": lane_id,
        "name": lane.name,
        "baseSnapshot": { "kind": "Snapshot", "id": head_snapshot },
        "headSnapshot": { "kind": "Snapshot", "id": head_snapshot },
        "createdAt": lane.created_at,
    });
    repo::write_registry_entry(repo::LANES_DIR, &lane_id, &entry)?;
    // Seed the new lane's independent head pointer at the current HEAD snapshot.
    repo::ensure_heads_migrated()?;
    repo::write_lane_head(&lane_id, &head_snapshot)?;

    Ok(CommandOutput {
        json: json!({
            "command": "lane create",
            "mocked": false,
            "status": "created",
            "repoId": repo_id,
            "object": entry
        }),
        human: format!("Created lane {} ({lane_id})", args.name),
    })
}

fn stack_create_output(args: StackCreateArgs) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    let tip =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    let mut options = StackOptions::new(
        args.name.clone(),
        tip,
        tip,
        Principal::system(),
        Visibility::Private,
    );
    options.created_at = repo::now_rfc3339();
    options.description = args.description.clone();
    let stack = to_io(create_stack(&store, options))?;
    let stack_id = stack.id.to_hex();

    let entry = json!({
        "kind": "Stack",
        "id": stack_id,
        "name": stack.name,
        "baseSnapshot": { "kind": "Snapshot", "id": tip.to_hex() },
        "headSnapshot": { "kind": "Snapshot", "id": tip.to_hex() },
        "description": stack.description,
        "createdAt": stack.created_at,
    });
    fs::create_dir_all(repo::registry_dir(repo::STACKS_DIR))?;
    repo::write_registry_entry(repo::STACKS_DIR, &stack_id, &entry)?;

    Ok(CommandOutput {
        json: json!({
            "command": "stack create",
            "mocked": false,
            "status": "created",
            "repoId": repo_id,
            "object": entry
        }),
        human: format!("Created stack {} ({stack_id})", args.name),
    })
}

fn stack_list_output() -> io::Result<CommandOutput> {
    let RepoContext { repo_id, .. } = open_repo()?;
    let entries = repo::list_registry_entries(repo::STACKS_DIR).unwrap_or_default();
    let mut human = String::new();
    for entry in &entries {
        let id = entry.get("id").and_then(Value::as_str).unwrap_or("?");
        let name = entry.get("name").and_then(Value::as_str).unwrap_or("?");
        human.push_str(&format!("{name}  {id}\n"));
    }
    if human.is_empty() {
        human.push_str("(no stacks)\n");
    }
    Ok(CommandOutput {
        json: json!({
            "command": "stack list",
            "mocked": false,
            "repoId": repo_id,
            "count": entries.len(),
            "objects": entries,
        }),
        human: human.trim_end().to_owned(),
    })
}

fn stack_show_output(args: StackShowArgs) -> io::Result<CommandOutput> {
    let RepoContext { repo_id, store, .. } = open_repo()?;
    let id = to_io(parse_object_id_hex(&args.stack_id))?;
    let stack = to_io(read_stack(&store, &id))?;
    let object = json!({
        "kind": "Stack",
        "id": stack.id.to_hex(),
        "name": stack.name,
        "baseSnapshot": { "kind": "Snapshot", "id": stack.base_snapshot.id.to_hex() },
        "headSnapshot": { "kind": "Snapshot", "id": stack.head_snapshot.id.to_hex() },
        "changes": stack.changes.iter().map(|c| json!({
            "kind": "Change",
            "id": c.id.to_hex(),
        })).collect::<Vec<_>>(),
        "dependencyStacks": stack.dependency_stacks.iter().map(|s| json!({
            "kind": "Stack",
            "id": s.id.to_hex(),
        })).collect::<Vec<_>>(),
        "description": stack.description,
        "createdAt": stack.created_at,
    });
    Ok(CommandOutput {
        json: json!({
            "command": "stack show",
            "mocked": false,
            "repoId": repo_id,
            "object": object,
        }),
        human: format!(
            "Stack {} ({})\n  base {}\n  head {}",
            stack.name,
            stack.id.to_hex(),
            stack.base_snapshot.id.to_hex(),
            stack.head_snapshot.id.to_hex()
        ),
    })
}

fn lane_list_output() -> io::Result<CommandOutput> {
    let RepoContext { repo_id, head, .. } = open_repo()?;
    repo::ensure_heads_migrated()?;

    let entries = repo::list_registry_entries(repo::LANES_DIR)?;
    let active_lane = head.lane.clone();
    let mut objects = Vec::with_capacity(entries.len());
    let mut human = String::new();

    for entry in entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let head_snapshot = repo::load_lane_head(&id)?.or_else(|| {
            entry
                .get("headSnapshot")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        let active = id == active_lane;
        objects.push(json!({
            "kind": "Lane",
            "id": id,
            "name": name,
            "active": active,
            "headSnapshot": head_snapshot
                .as_ref()
                .map(|snapshot| json!({ "kind": "Snapshot", "id": snapshot }))
                .unwrap_or(Value::Null),
        }));
        let marker = if active { "*" } else { " " };
        let snapshot_label = head_snapshot
            .as_deref()
            .map(|snapshot| &snapshot[..snapshot.len().min(12)])
            .unwrap_or("(none)");
        human.push_str(&format!("{marker} {id}  {name}  {snapshot_label}\n"));
    }

    if objects.is_empty() {
        human = "No lanes recorded".to_owned();
    }

    Ok(CommandOutput {
        json: json!({
            "command": "lane list",
            "mocked": false,
            "repoId": repo_id,
            "activeLane": { "kind": "Lane", "id": active_lane },
            "count": objects.len(),
            "objects": objects,
        }),
        human: human.trim_end().to_owned(),
    })
}

fn lane_switch_output(args: LaneSwitchArgs) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    repo::ensure_heads_migrated()?;

    if !repo::lane_exists(&args.lane_id) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("lane `{}` does not exist", args.lane_id),
        ));
    }

    let current_snapshot =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    // Refuse to switch with a dirty working tree so we never lose uncommitted edits.
    if worktree_is_dirty(&store, &repo_id, &current_snapshot)? {
        return Err(io::Error::other(
            "working tree has uncommitted changes; commit or discard them before switching lanes",
        ));
    }

    let target_snapshot_hex = repo::load_lane_head(&args.lane_id)?
        .ok_or_else(|| io::Error::other(format!("lane `{}` has no head snapshot", args.lane_id)))?;
    let target_snapshot = target_snapshot_hex
        .parse::<ObjectId>()
        .map_err(|error| io::Error::other(format!("invalid lane head snapshot id: {error}")))?;

    // Restore target snapshot into the working tree without touching `.sorrel/`.
    // Snapshots never contain `.sorrel`, but restore does not delete extras, so
    // remove paths present in the current HEAD that are absent from the target.
    restore_worktree_to_snapshot(&store, &current_snapshot, &target_snapshot)?;

    repo::write_head(&repo::Head {
        lane: args.lane_id.clone(),
        snapshot: target_snapshot_hex.clone(),
    })?;

    Ok(CommandOutput {
        json: json!({
            "command": "lane switch",
            "mocked": false,
            "status": "switched",
            "repoId": repo_id,
            "lane": { "kind": "Lane", "id": args.lane_id },
            "headSnapshot": { "kind": "Snapshot", "id": target_snapshot_hex },
        }),
        human: format!(
            "Switched to lane {} at {}",
            args.lane_id,
            &target_snapshot_hex[..target_snapshot_hex.len().min(12)]
        ),
    })
}

fn lane_submit_output(args: LaneSubmitArgs) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    let snapshot_id = head_snapshot_id(&head)?
        .ok_or_else(|| io::Error::other("HEAD has no snapshot to submit"))?;
    let source_lane = head.lane.clone();
    let snapshot_hex = snapshot_id.to_hex();

    let remotes = repo::load_remotes()?;
    let (remote_name, remote) = remotes.resolve(Some(&args.remote))?;

    let do_push = args.push && !args.no_push;
    let mut uploaded = 0usize;
    if do_push {
        let push_result = sync::push(&store, &remote, &remote_name, "HEAD", &snapshot_id, None)?;
        uploaded = push_result.uploaded;
    }

    let project_id = match args.project_id {
        Some(id) => id,
        None => hub::ensure_project(&remote.url, &args.organization_id, &repo_id)?,
    };

    let title = args
        .title
        .unwrap_or_else(|| format!("Submit {source_lane}"));
    let result = hub::lane_submit(
        &remote,
        &project_id,
        &title,
        &source_lane,
        &snapshot_hex,
        &args.target_lane,
    )?;

    let proposal_id = result
        .proposal
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned();
    let status = result
        .proposal
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("open");

    Ok(CommandOutput {
        json: json!({
            "command": "lane submit",
            "mocked": false,
            "status": if result.reused { "reused" } else { "submitted" },
            "remote": remote_name,
            "pushed": do_push,
            "uploaded": uploaded,
            "projectId": result.project_id,
            "proposal": result.proposal,
            "reused": result.reused,
            "lane": { "kind": "Lane", "id": source_lane },
            "snapshot": { "kind": "Snapshot", "id": snapshot_hex },
        }),
        human: format!(
            "{} proposal {proposal_id} ({status}) on project {} via {remote_name}",
            if result.reused { "Reused" } else { "Submitted" },
            result.project_id,
        ),
    })
}

fn git_import_output(args: GitImportArgs) -> io::Result<CommandOutput> {
    let git_path = if args.path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        args.path.clone()
    };

    let created_workspace = if !repo::is_initialized() {
        let _ = init_output()?;
        true
    } else {
        false
    };

    let RepoContext {
        store,
        repo_id,
        head,
        ..
    } = open_repo()?;

    if !args.force {
        if repo::git_map_path().is_file() {
            return Err(io::Error::other(
                "`.sorrel/git-map.json` already exists; pass --force to re-import",
            ));
        }
        // Fresh init leaves an empty HEAD while the Git working tree has files —
        // that is expected. Only refuse a dirty tree when the workspace already
        // existed (unrelated uncommitted Sorrel edits).
        if !created_workspace {
            if let Some(base) = head_snapshot_id(&head)? {
                if worktree_is_dirty(&store, &repo_id, &base)? {
                    return Err(io::Error::other(
                        "working tree has uncommitted changes; commit, discard, or pass --force",
                    ));
                }
            }
        }
    }

    let mut options = GitImportOptions::new(&git_path, repo_id.clone());
    options.git_ref = args.git_ref.clone();
    options.limit = args.limit;

    let imported = to_io(git_import(&store, options))?;
    let head_hex = imported.head_snapshot.to_hex();

    for commit in &imported.commits {
        repo::append_changes_index(&repo::ChangesIndexEntry {
            snapshot: commit.snapshot_id.to_hex(),
            change: commit.change_id.to_hex(),
        })?;
    }

    let map_value = json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "GitImportMap",
        "ref": args.git_ref,
        "commits": imported
            .commits
            .iter()
            .map(|c| {
                json!({
                    "gitSha": c.git_sha,
                    "snapshot": c.snapshot_id.to_hex(),
                    "change": c.change_id.to_hex(),
                    "message": c.message,
                })
            })
            .collect::<Vec<_>>(),
        "gitToSnapshot": imported
            .git_to_snapshot
            .iter()
            .map(|(sha, id)| (sha.clone(), Value::String(id.to_hex())))
            .collect::<serde_json::Map<String, Value>>(),
    });
    repo::write_json_atomic(&repo::git_map_path(), &map_value)?;

    let previous = head_snapshot_id(&head)?.unwrap_or(imported.empty_base_snapshot);
    restore_worktree_to_snapshot(&store, &previous, &imported.head_snapshot)?;

    let lane = head.lane.clone();
    repo::write_head(&repo::Head {
        lane: lane.clone(),
        snapshot: head_hex.clone(),
    })?;

    let count = imported.commits.len();
    Ok(CommandOutput {
        json: json!({
            "command": "git import",
            "mocked": false,
            "status": "imported",
            "repoId": repo_id,
            "createdWorkspace": created_workspace,
            "gitPath": git_path,
            "ref": args.git_ref,
            "limit": args.limit,
            "importedCommits": count,
            "headSnapshot": { "kind": "Snapshot", "id": head_hex },
            "gitMap": repo::GIT_MAP_FILE,
            "commits": imported.commits.iter().map(|c| json!({
                "gitSha": c.git_sha,
                "snapshot": c.snapshot_id.to_hex(),
                "change": c.change_id.to_hex(),
                "message": c.message,
            })).collect::<Vec<_>>(),
        }),
        human: format!(
            "Imported {count} commit(s) from Git ({}) into Sorrel; HEAD → {head_hex}",
            args.git_ref
        ),
    })
}

fn git_export_output(args: GitExportArgs) -> io::Result<CommandOutput> {
    let git_path = if args.path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        args.path.clone()
    };

    let RepoContext {
        store,
        repo_id,
        head,
        ..
    } = open_repo()?;

    let tip = if let Some(hex) = args.snapshot.as_deref() {
        to_io(parse_object_id_hex(hex))?
    } else {
        head_snapshot_id(&head)?
            .ok_or_else(|| io::Error::other("HEAD has no snapshot to export"))?
    };

    let mut snapshot_to_git: std::collections::BTreeMap<ObjectId, String> =
        std::collections::BTreeMap::new();
    let mut existing_map: Option<Value> = None;
    if repo::git_map_path().is_file() {
        let bytes = fs::read(repo::git_map_path())?;
        let map: Value = serde_json::from_slice(&bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        existing_map = Some(map.clone());
        if let Some(obj) = map.get("gitToSnapshot").and_then(Value::as_object) {
            for (sha, snap) in obj {
                if let Some(hex) = snap.as_str() {
                    if let Ok(id) = parse_object_id_hex(hex) {
                        snapshot_to_git.insert(id, sha.clone());
                    }
                }
            }
        }
        if let Some(commits) = map.get("commits").and_then(Value::as_array) {
            for commit in commits {
                let sha = commit.get("gitSha").and_then(Value::as_str);
                let snap = commit.get("snapshot").and_then(Value::as_str);
                if let (Some(sha), Some(hex)) = (sha, snap) {
                    if let Ok(id) = parse_object_id_hex(hex) {
                        snapshot_to_git.insert(id, sha.to_owned());
                    }
                }
            }
        }
    }

    // Refuse to clobber an existing non-empty branch unless --force or we have a map.
    if !args.force && snapshot_to_git.is_empty() && git_branch_exists(&git_path, &args.branch) {
        return Err(io::Error::other(format!(
            "Git branch '{}' already exists at {}; pass --force to overwrite",
            args.branch,
            git_path.display()
        )));
    }

    let mut options = GitExportOptions::new(&git_path, tip);
    options.branch = args.branch.clone();
    options.snapshot_to_git = snapshot_to_git;

    let exported = to_io(git_export(&store, options))?;
    let created = exported.commits.iter().filter(|c| c.created).count();

    let commits_json: Vec<Value> = exported
        .commits
        .iter()
        .map(|c| {
            json!({
                "gitSha": c.git_sha,
                "snapshot": c.snapshot_id.to_hex(),
                "message": c.message,
                "created": c.created,
            })
        })
        .collect();

    let git_to_snapshot: serde_json::Map<String, Value> = exported
        .snapshot_to_git
        .iter()
        .map(|(id, sha)| (sha.clone(), Value::String(id.to_hex())))
        .collect();

    let map_value = json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "GitExportMap",
        "branch": exported.branch,
        "commits": commits_json,
        "gitToSnapshot": git_to_snapshot,
        "previous": existing_map,
    });
    repo::write_json_atomic(&repo::git_map_path(), &map_value)?;

    Ok(CommandOutput {
        json: json!({
            "command": "git export",
            "mocked": false,
            "status": "exported",
            "repoId": repo_id,
            "gitPath": git_path,
            "branch": exported.branch,
            "tipSnapshot": { "kind": "Snapshot", "id": tip.to_hex() },
            "headGitSha": exported.head_git_sha,
            "exportedCommits": exported.commits.len(),
            "createdCommits": created,
            "gitMap": repo::GIT_MAP_FILE,
            "commits": exported.commits.iter().map(|c| json!({
                "gitSha": c.git_sha,
                "snapshot": c.snapshot_id.to_hex(),
                "message": c.message,
                "created": c.created,
            })).collect::<Vec<_>>(),
        }),
        human: format!(
            "Exported {} snapshot(s) ({} new) to Git branch '{}' at {}; tip {}",
            exported.commits.len(),
            created,
            exported.branch,
            git_path.display(),
            exported.head_git_sha
        ),
    })
}

fn git_branch_exists(git_path: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(git_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Bidirectional fast-forward sync between the workspace and a mirrored Git
/// branch (colocated `.git` or a separate mirror directory).
///
/// - Git moved, Sorrel did not → import the new commits and fast-forward HEAD.
/// - Sorrel moved, Git did not → export the new snapshots and advance the branch.
/// - Both moved → import the Git tip, park it on lane `git/<branch>`, and ask
///   the user to `sorrel merge` it; the next sync pushes the merge result.
fn git_sync_output(args: GitSyncArgs) -> io::Result<CommandOutput> {
    let git_path = if args.path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        args.path.clone()
    };

    let RepoContext {
        store,
        repo_id,
        head,
    } = open_repo()?;
    repo::ensure_heads_migrated()?;
    let head_id = head_snapshot_id(&head)?;

    let sha_map = load_git_sha_map()?;
    let git_tip = git_branch_tip(&git_path, &args.branch);

    match (git_tip.as_deref(), head_id) {
        (None, None) => Err(io::Error::other(
            "nothing to sync: the Git branch does not exist and Sorrel HEAD is unborn",
        )),
        // Sorrel has history, the Git branch does not exist yet → bootstrap/push.
        (None, Some(tip)) => {
            git_sync_push(&store, &repo_id, &git_path, &args.branch, tip, &sha_map)
        }
        // Git has history, Sorrel HEAD is unborn → pull everything.
        (Some(tip_sha), None) => {
            let imported = git_sync_import(&store, &repo_id, &git_path, &args.branch, &sha_map)?;
            write_git_sync_map(&args.branch, &imported.git_to_snapshot)?;
            git_sync_finish_pull(
                &store,
                &repo_id,
                &head,
                Some(imported.empty_base_snapshot),
                imported.head_snapshot,
                args.force,
            )?;
            Ok(git_sync_pulled_output(
                &repo_id,
                &git_path,
                &args.branch,
                tip_sha,
                &imported.head_snapshot.to_hex(),
                &imported.commits,
            ))
        }
        (Some(tip_sha), Some(head_snapshot)) => {
            if let Some(&mapped) = sha_map.get(tip_sha) {
                if mapped == head_snapshot {
                    return Ok(CommandOutput {
                        json: json!({
                            "command": "git sync",
                            "mocked": false,
                            "status": "up-to-date",
                            "repoId": repo_id,
                            "gitPath": git_path,
                            "branch": args.branch,
                            "headSnapshot": { "kind": "Snapshot", "id": head_snapshot.to_hex() },
                            "headGitSha": tip_sha,
                        }),
                        human: format!(
                            "Git mirror up to date (branch '{}' at {})",
                            args.branch,
                            &tip_sha[..12.min(tip_sha.len())]
                        ),
                    });
                }
                // Sorrel moved past the mapped Git tip → push.
                if to_io(is_descendant(&store, mapped, head_snapshot))? {
                    return git_sync_push(
                        &store,
                        &repo_id,
                        &git_path,
                        &args.branch,
                        head_snapshot,
                        &sha_map,
                    );
                }
                // The mapped Git tip is ahead of HEAD → fast-forward locally.
                // A parentless empty-root HEAD (fresh `sorrel init`) may be
                // fast-forwarded past even though the histories are unrelated.
                if to_io(is_descendant(&store, head_snapshot, mapped))?
                    || snapshot_is_empty_root(&store, &head_snapshot)?
                {
                    git_sync_finish_pull(
                        &store,
                        &repo_id,
                        &head,
                        Some(head_snapshot),
                        mapped,
                        args.force,
                    )?;
                    return Ok(git_sync_pulled_output(
                        &repo_id,
                        &git_path,
                        &args.branch,
                        tip_sha,
                        &mapped.to_hex(),
                        &[],
                    ));
                }
                let (lane_id, lane_name) = park_git_lane(&store, &args.branch, &mapped)?;
                return Ok(git_sync_diverged_output(
                    &repo_id,
                    &git_path,
                    &args.branch,
                    &head_snapshot,
                    &mapped,
                    &lane_id,
                    &lane_name,
                    0,
                ));
            }

            // The Git tip is unmapped → Git gained new commits; import them.
            let imported = git_sync_import(&store, &repo_id, &git_path, &args.branch, &sha_map)?;
            write_git_sync_map(&args.branch, &imported.git_to_snapshot)?;
            let theirs = imported.head_snapshot;
            if to_io(is_descendant(&store, head_snapshot, theirs))?
                || snapshot_is_empty_root(&store, &head_snapshot)?
            {
                git_sync_finish_pull(
                    &store,
                    &repo_id,
                    &head,
                    Some(head_snapshot),
                    theirs,
                    args.force,
                )?;
                Ok(git_sync_pulled_output(
                    &repo_id,
                    &git_path,
                    &args.branch,
                    tip_sha,
                    &theirs.to_hex(),
                    &imported.commits,
                ))
            } else {
                let (lane_id, lane_name) = park_git_lane(&store, &args.branch, &theirs)?;
                Ok(git_sync_diverged_output(
                    &repo_id,
                    &git_path,
                    &args.branch,
                    &head_snapshot,
                    &theirs,
                    &lane_id,
                    &lane_name,
                    imported.commits.len(),
                ))
            }
        }
    }
}

/// True when `id` is a parentless snapshot of an empty tree — the state a
/// fresh `sorrel init` leaves behind. Such a HEAD carries no work, so a pull
/// may fast-forward past it even though the imported history is unrelated.
fn snapshot_is_empty_root(store: &FileObjectStore, id: &ObjectId) -> io::Result<bool> {
    let snapshot = to_io(read_snapshot(store, id))?;
    if !snapshot.parents.is_empty() {
        return Ok(false);
    }
    let files = to_io(read_snapshot_files(store, id))?;
    Ok(files.is_empty())
}

/// Incremental Git → Sorrel import of the mirrored branch; known commits are
/// skipped and new commits are appended to the changes index for `sorrel log`.
fn git_sync_import(
    store: &FileObjectStore,
    repo_id: &str,
    git_path: &Path,
    branch: &str,
    sha_map: &std::collections::BTreeMap<String, ObjectId>,
) -> io::Result<ImportResult> {
    let mut options = GitImportOptions::new(git_path, repo_id.to_owned());
    options.git_ref = format!("refs/heads/{branch}");
    options.known_commits = sha_map.clone();
    let imported = to_io(git_import(store, options))?;
    for commit in &imported.commits {
        repo::append_changes_index(&repo::ChangesIndexEntry {
            snapshot: commit.snapshot_id.to_hex(),
            change: commit.change_id.to_hex(),
        })?;
    }
    Ok(imported)
}

/// Fast-forwards HEAD (and the working tree) to `target` after a pull.
///
/// The working tree is left untouched when it already matches `target`
/// (typical colocated case: the user just committed via Git). It is restored
/// when it cleanly matches `baseline` (typical mirror-directory case). Any
/// other state is an uncommitted-changes error unless `force` is set.
fn git_sync_finish_pull(
    store: &FileObjectStore,
    repo_id: &str,
    head: &repo::Head,
    baseline: Option<ObjectId>,
    target: ObjectId,
    force: bool,
) -> io::Result<()> {
    let current = materialize_worktree(store, repo_id, None, &[], None)?;
    let matches_target = to_io(snapshot_diff(store, &target, &current))?.is_empty();
    if !matches_target {
        let clean = match baseline {
            Some(base) => to_io(snapshot_diff(store, &base, &current))?.is_empty(),
            None => false,
        };
        if !clean && !force {
            return Err(io::Error::other(
                "working tree has uncommitted changes; commit or discard them, or pass --force",
            ));
        }
        restore_worktree_to_snapshot(store, &current, &target)?;
    }
    repo::write_head(&repo::Head {
        lane: head.lane.clone(),
        snapshot: target.to_hex(),
    })
}

/// Exports Sorrel history onto the mirrored Git branch and refreshes the map.
fn git_sync_push(
    store: &FileObjectStore,
    repo_id: &str,
    git_path: &Path,
    branch: &str,
    tip: ObjectId,
    sha_map: &std::collections::BTreeMap<String, ObjectId>,
) -> io::Result<CommandOutput> {
    let snapshot_to_git: std::collections::BTreeMap<ObjectId, String> =
        sha_map.iter().map(|(sha, id)| (*id, sha.clone())).collect();
    let mut options = GitExportOptions::new(git_path, tip);
    options.branch = branch.to_owned();
    options.snapshot_to_git = snapshot_to_git;
    let exported = to_io(git_export(store, options))?;

    let mut merged = sha_map.clone();
    for (id, sha) in &exported.snapshot_to_git {
        merged.insert(sha.clone(), *id);
    }
    write_git_sync_map(branch, &merged)?;

    // Colocated checkouts keep the synced branch checked out; refresh the Git
    // index so `git status` stays clean after the ref moved (worktree already
    // matches the exported content).
    if git_checked_out_branch(git_path).as_deref() == Some(branch) {
        git_reset_index(git_path, &exported.head_git_sha);
    }

    let created = exported.commits.iter().filter(|c| c.created).count();
    let sha = exported.head_git_sha.clone();
    Ok(CommandOutput {
        json: json!({
            "command": "git sync",
            "mocked": false,
            "status": "pushed",
            "repoId": repo_id,
            "gitPath": git_path,
            "branch": branch,
            "headSnapshot": { "kind": "Snapshot", "id": tip.to_hex() },
            "headGitSha": sha,
            "exportedCommits": exported.commits.len(),
            "createdCommits": created,
            "gitMap": repo::GIT_MAP_FILE,
        }),
        human: format!(
            "Pushed {created} new commit(s) to Git branch '{branch}'; tip {}",
            &sha[..12.min(sha.len())]
        ),
    })
}

fn git_sync_pulled_output(
    repo_id: &str,
    git_path: &Path,
    branch: &str,
    tip_sha: &str,
    head_hex: &str,
    commits: &[ImportedCommit],
) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": "git sync",
            "mocked": false,
            "status": "pulled",
            "repoId": repo_id,
            "gitPath": git_path,
            "branch": branch,
            "headSnapshot": { "kind": "Snapshot", "id": head_hex },
            "headGitSha": tip_sha,
            "importedCommits": commits.len(),
            "gitMap": repo::GIT_MAP_FILE,
            "commits": commits.iter().map(|c| json!({
                "gitSha": c.git_sha,
                "snapshot": c.snapshot_id.to_hex(),
                "change": c.change_id.to_hex(),
                "message": c.message,
            })).collect::<Vec<_>>(),
        }),
        human: format!(
            "Pulled {} Git commit(s) from branch '{branch}'; HEAD → {}",
            commits.len(),
            &head_hex[..12.min(head_hex.len())]
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn git_sync_diverged_output(
    repo_id: &str,
    git_path: &Path,
    branch: &str,
    ours: &ObjectId,
    theirs: &ObjectId,
    lane_id: &str,
    lane_name: &str,
    imported_commits: usize,
) -> CommandOutput {
    CommandOutput {
        json: json!({
            "command": "git sync",
            "mocked": false,
            "status": "diverged",
            "repoId": repo_id,
            "gitPath": git_path,
            "branch": branch,
            "headSnapshot": { "kind": "Snapshot", "id": ours.to_hex() },
            "theirsSnapshot": { "kind": "Snapshot", "id": theirs.to_hex() },
            "importedCommits": imported_commits,
            "lane": { "kind": "Lane", "id": lane_id, "name": lane_name },
            "gitMap": repo::GIT_MAP_FILE,
        }),
        human: format!(
            "Sorrel and Git histories diverged; Git tip parked on lane {lane_id} ({lane_name}). \
             Run `sorrel merge {lane_id}`, then `sorrel git sync` again."
        ),
    }
}

/// Parks the imported Git tip on lane `git/<branch>` so a divergent mirror can
/// be resolved with a normal `sorrel merge`. Reuses the lane when it exists.
fn park_git_lane(
    store: &FileObjectStore,
    branch: &str,
    theirs: &ObjectId,
) -> io::Result<(String, String)> {
    let name = format!("git/{branch}");
    let theirs_hex = theirs.to_hex();
    repo::ensure_heads_migrated()?;

    for entry in repo::list_registry_entries(repo::LANES_DIR)? {
        if entry.get("name").and_then(Value::as_str) == Some(name.as_str()) {
            if let Some(id) = entry.get("id").and_then(Value::as_str).map(str::to_owned) {
                let mut updated = entry.clone();
                updated["headSnapshot"] = json!({ "kind": "Snapshot", "id": theirs_hex });
                repo::write_registry_entry(repo::LANES_DIR, &id, &updated)?;
                repo::write_lane_head(&id, &theirs_hex)?;
                return Ok((id, name));
            }
        }
    }

    let mut options = LaneOptions::new(
        name.clone(),
        *theirs,
        *theirs,
        Principal::system(),
        Visibility::Private,
    );
    options.created_at = repo::now_rfc3339();
    let lane = to_io(create_lane(store, options))?;
    let lane_id = lane.id.to_hex();
    let entry = json!({
        "kind": "Lane",
        "id": lane_id,
        "name": lane.name,
        "baseSnapshot": { "kind": "Snapshot", "id": theirs_hex },
        "headSnapshot": { "kind": "Snapshot", "id": theirs_hex },
        "createdAt": lane.created_at,
    });
    repo::write_registry_entry(repo::LANES_DIR, &lane_id, &entry)?;
    repo::write_lane_head(&lane_id, &theirs_hex)?;
    Ok((lane_id, name))
}

/// Loads `.sorrel/git-map.json` into a Git SHA → snapshot id map. Reads the
/// `gitToSnapshot` object plus per-commit entries from import/export/sync maps.
fn load_git_sha_map() -> io::Result<std::collections::BTreeMap<String, ObjectId>> {
    let mut map = std::collections::BTreeMap::new();
    if !repo::git_map_path().is_file() {
        return Ok(map);
    }
    let bytes = fs::read(repo::git_map_path())?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if let Some(obj) = value.get("gitToSnapshot").and_then(Value::as_object) {
        for (sha, snap) in obj {
            if let Some(hex) = snap.as_str() {
                if let Ok(id) = parse_object_id_hex(hex) {
                    map.insert(sha.clone(), id);
                }
            }
        }
    }
    if let Some(commits) = value.get("commits").and_then(Value::as_array) {
        for commit in commits {
            let sha = commit.get("gitSha").and_then(Value::as_str);
            let snap = commit.get("snapshot").and_then(Value::as_str);
            if let (Some(sha), Some(hex)) = (sha, snap) {
                if let Ok(id) = parse_object_id_hex(hex) {
                    map.insert(sha.to_owned(), id);
                }
            }
        }
    }
    Ok(map)
}

/// Atomically writes the merged SHA ↔ snapshot map after a sync operation.
fn write_git_sync_map(
    branch: &str,
    map: &std::collections::BTreeMap<String, ObjectId>,
) -> io::Result<()> {
    let git_to_snapshot: serde_json::Map<String, Value> = map
        .iter()
        .map(|(sha, id)| (sha.clone(), Value::String(id.to_hex())))
        .collect();
    let value = json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "GitSyncMap",
        "branch": branch,
        "gitToSnapshot": git_to_snapshot,
    });
    repo::write_json_atomic(&repo::git_map_path(), &value)
}

/// Resolves the full SHA of `refs/heads/<branch>`, or `None` when the branch
/// (or repository) does not exist.
fn git_branch_tip(git_path: &Path, branch: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(git_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        None
    } else {
        Some(sha)
    }
}

/// Returns the branch checked out at `git_path`, if HEAD is a symbolic ref.
fn git_checked_out_branch(git_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(git_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// Best-effort `git reset --mixed <sha>` to refresh a colocated index after the
/// checked-out branch ref moved (the worktree already matches the new commit).
fn git_reset_index(git_path: &Path, sha: &str) {
    let _ = std::process::Command::new("git")
        .args(["reset", "--quiet", "--mixed", sha])
        .current_dir(git_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn merge_output(args: MergeArgs) -> io::Result<CommandOutput> {
    match (args.abort, args.r#continue, args.lane_id.as_deref()) {
        (true, true, _) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "specify either --abort or --continue, not both",
        )),
        (true, false, None) => merge_abort_output(),
        (true, false, Some(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "specify either a lane id or --abort, not both",
        )),
        (false, true, None) => merge_continue_output(),
        (false, true, Some(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "specify either a lane id or --continue, not both",
        )),
        (false, false, Some(lane_id)) => merge_lane_output(lane_id),
        (false, false, None) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing lane id (or pass --abort / --continue)",
        )),
    }
}

fn merge_abort_output() -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;

    if !repo::merge_in_progress() {
        return Err(io::Error::other(
            "no merge in progress (`.sorrel/MERGE_STATE` not found)",
        ));
    }

    let head_snapshot =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    // Conflicted merges leave marker-annotated files in the worktree but do not
    // advance HEAD. Restore HEAD's snapshot and drop MERGE_STATE.
    let dirty_snapshot = materialize_worktree(&store, &repo_id, None, &[], None)?;
    restore_worktree_to_snapshot(&store, &dirty_snapshot, &head_snapshot)?;
    repo::clear_merge_state()?;

    Ok(CommandOutput {
        json: json!({
            "command": "merge",
            "mocked": false,
            "status": "aborted",
            "repoId": repo_id,
            "headSnapshot": { "kind": "Snapshot", "id": head_snapshot.to_hex() },
        }),
        human: format!(
            "Merge aborted; restored working tree to {}",
            &head_snapshot.to_hex()[..12]
        ),
    })
}

fn merge_continue_output() -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    repo::ensure_heads_migrated()?;

    let state = repo::load_merge_state_record()?.ok_or_else(|| {
        io::Error::other("no merge in progress (`.sorrel/MERGE_STATE` not found)")
    })?;

    if state.ours_snapshot.is_empty() || state.theirs_snapshot.is_empty() {
        return Err(io::Error::other(
            "MERGE_STATE is missing ours/theirs snapshots; abort and re-merge",
        ));
    }

    let ours_id = state
        .ours_snapshot
        .parse::<ObjectId>()
        .map_err(|error| io::Error::other(format!("invalid ours snapshot id: {error}")))?;
    let theirs_id = state
        .theirs_snapshot
        .parse::<ObjectId>()
        .map_err(|error| io::Error::other(format!("invalid theirs snapshot id: {error}")))?;

    let head_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;
    if head_id != ours_id {
        return Err(io::Error::other(
            "HEAD moved since the conflicted merge started; abort and re-merge",
        ));
    }

    let remaining = conflict_marker_paths()?;
    if !remaining.is_empty() {
        return Err(io::Error::other(format!(
            "unresolved conflict markers in: {}; remove markers before --continue",
            remaining.join(", ")
        )));
    }

    let message = if state.message.is_empty() {
        format!("merge {}", state.lane)
    } else {
        state.message.clone()
    };

    let result_snapshot = materialize_worktree(
        &store,
        &repo_id,
        Some(message.clone()),
        &[ours_id, theirs_id],
        None,
    )?;

    let change = to_io(create_change(
        &store,
        ours_id,
        result_snapshot,
        ChangeOptions::new(Principal::system(), message.clone()),
    ))?;

    let result_hex = result_snapshot.to_hex();
    repo::write_head(&repo::Head {
        lane: head.lane.clone(),
        snapshot: result_hex.clone(),
    })?;

    let change_id = change.id.to_hex();
    repo::append_changes_index(&repo::ChangesIndexEntry {
        snapshot: result_hex.clone(),
        change: change_id.clone(),
    })?;
    repo::clear_merge_state()?;

    let (changes, total) = diff_json(&change.diff);
    let lane_id = if state.lane.is_empty() {
        "unknown".to_owned()
    } else {
        state.lane.clone()
    };

    Ok(CommandOutput {
        json: json!({
            "command": "merge",
            "mocked": false,
            "status": "merged",
            "fastForward": false,
            "continued": true,
            "repoId": repo_id,
            "lane": { "kind": "Lane", "id": lane_id },
            "baseSnapshot": { "kind": "Snapshot", "id": state.base_snapshot },
            "oursSnapshot": { "kind": "Snapshot", "id": state.ours_snapshot },
            "theirsSnapshot": { "kind": "Snapshot", "id": state.theirs_snapshot },
            "headSnapshot": { "kind": "Snapshot", "id": result_hex },
            "mergeResult": { "kind": "MergeResult", "id": state.merge_result },
            "change": {
                "kind": "Change",
                "id": change_id,
                "message": message,
                "baseSnapshot": { "kind": "Snapshot", "id": ours_id.to_hex() },
                "resultingSnapshot": { "kind": "Snapshot", "id": result_hex },
                "changedPaths": total,
                "diff": changes,
            },
        }),
        human: format!("Continued merge of {lane_id} ({change_id}; {total} path(s))"),
    })
}

/// Returns sorted worktree paths that still contain Git-style conflict markers.
fn conflict_marker_paths() -> io::Result<Vec<String>> {
    let mut paths = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if name == repo::SORREL_DIR || name == ".git" {
                continue;
            }
            if path.is_dir() {
                walk(&path, out)?;
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            if text.contains("<<<<<<< ours") || text.contains(">>>>>>> theirs") {
                let relative = path
                    .strip_prefix(".")
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .trim_start_matches("./")
                    .to_owned();
                out.push(relative);
            }
        }
        Ok(())
    }
    walk(Path::new("."), &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn merge_lane_output(lane_id: &str) -> io::Result<CommandOutput> {
    let RepoContext {
        repo_id,
        head,
        store,
    } = open_repo()?;
    repo::ensure_heads_migrated()?;

    if repo::merge_in_progress() {
        return Err(io::Error::other(
            "a merge is already in progress; resolve conflicts or run `sorrel merge --abort`",
        ));
    }

    if !repo::lane_exists(lane_id) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("lane `{lane_id}` does not exist"),
        ));
    }

    if lane_id == head.lane {
        return Err(io::Error::other(format!(
            "cannot merge lane `{lane_id}` into itself"
        )));
    }

    let ours_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no base snapshot"))?;

    if worktree_is_dirty(&store, &repo_id, &ours_id)? {
        return Err(io::Error::other(
            "working tree has uncommitted changes; commit or discard them before merging",
        ));
    }

    let theirs_hex = repo::load_lane_head(lane_id)?
        .ok_or_else(|| io::Error::other(format!("lane `{lane_id}` has no head snapshot")))?;
    let theirs_id = theirs_hex
        .parse::<ObjectId>()
        .map_err(|error| io::Error::other(format!("invalid lane head snapshot id: {error}")))?;

    if ours_id == theirs_id {
        return Err(io::Error::other(format!(
            "nothing to merge: active lane and `{lane_id}` already share the same head"
        )));
    }

    let base_id = to_io(merge_base(&store, ours_id, theirs_id))?.ok_or_else(|| {
        io::Error::other(format!(
            "refusing to merge unrelated histories (no merge base with `{lane_id}`)"
        ))
    })?;

    // Fast-forward: active lane has not diverged; just advance to theirs.
    if base_id == ours_id {
        restore_worktree_to_snapshot(&store, &ours_id, &theirs_id)?;
        repo::write_head(&repo::Head {
            lane: head.lane.clone(),
            snapshot: theirs_hex.clone(),
        })?;

        return Ok(CommandOutput {
            json: json!({
                "command": "merge",
                "mocked": false,
                "status": "merged",
                "fastForward": true,
                "repoId": repo_id,
                "lane": { "kind": "Lane", "id": lane_id },
                "baseSnapshot": { "kind": "Snapshot", "id": base_id.to_hex() },
                "oursSnapshot": { "kind": "Snapshot", "id": ours_id.to_hex() },
                "theirsSnapshot": { "kind": "Snapshot", "id": theirs_hex },
                "headSnapshot": { "kind": "Snapshot", "id": theirs_hex },
                "change": Value::Null,
            }),
            human: format!(
                "Fast-forwarded to {} (merged {lane_id})",
                &theirs_hex[..theirs_hex.len().min(12)]
            ),
        });
    }

    let message = format!("merge {lane_id}");
    let mut merge_options =
        MergeOptions::new(Principal::system(), repo_id.clone(), message.clone());
    merge_options.created_at = repo::now_rfc3339();
    let merge_result = to_io(merge_snapshots(
        &store,
        &base_id,
        &ours_id,
        &theirs_id,
        &merge_options,
    ))?;

    let Some(result_snapshot) = merge_result.merged_snapshot else {
        // Conflicted: write marker-annotated content into the worktree for
        // each conflicted path; do not advance HEAD.
        let paths = write_conflict_markers(&store, &merge_result, &base_id, &ours_id, &theirs_id)?;
        repo::write_merge_state_record(&repo::MergeState {
            merge_result: merge_result.id.to_hex(),
            lane: lane_id.to_owned(),
            base_snapshot: base_id.to_hex(),
            ours_snapshot: ours_id.to_hex(),
            theirs_snapshot: theirs_id.to_hex(),
            message,
        })?;

        let listed = paths.join(", ");
        return Err(io::Error::other(format!(
            "merge conflicts in: {listed}; fix markers then `sorrel merge --continue`, or `sorrel merge --abort`"
        )));
    };

    // Clean three-way merge: restore merged tree, advance HEAD, record Change.
    restore_worktree_to_snapshot(&store, &ours_id, &result_snapshot)?;

    let change = to_io(create_change(
        &store,
        ours_id,
        result_snapshot,
        ChangeOptions::new(Principal::system(), message.clone()),
    ))?;

    let result_hex = result_snapshot.to_hex();
    repo::write_head(&repo::Head {
        lane: head.lane.clone(),
        snapshot: result_hex.clone(),
    })?;

    let change_id = change.id.to_hex();
    repo::append_changes_index(&repo::ChangesIndexEntry {
        snapshot: result_hex.clone(),
        change: change_id.clone(),
    })?;

    let (changes, total) = diff_json(&change.diff);

    Ok(CommandOutput {
        json: json!({
            "command": "merge",
            "mocked": false,
            "status": "merged",
            "fastForward": false,
            "repoId": repo_id,
            "lane": { "kind": "Lane", "id": lane_id },
            "baseSnapshot": { "kind": "Snapshot", "id": base_id.to_hex() },
            "oursSnapshot": { "kind": "Snapshot", "id": ours_id.to_hex() },
            "theirsSnapshot": { "kind": "Snapshot", "id": theirs_hex },
            "headSnapshot": { "kind": "Snapshot", "id": result_hex },
            "mergeResult": { "kind": "MergeResult", "id": merge_result.id.to_hex() },
            "change": {
                "kind": "Change",
                "id": change_id,
                "message": message,
                "baseSnapshot": { "kind": "Snapshot", "id": ours_id.to_hex() },
                "resultingSnapshot": { "kind": "Snapshot", "id": result_snapshot.to_hex() },
                "changedPaths": total,
                "diff": changes,
            },
        }),
        human: format!("Merged {lane_id} ({change_id}; {total} path(s))"),
    })
}

/// Writes marker-annotated content into the working tree for every conflicted
/// path of `merge_result` and returns the sorted list of those paths.
///
/// The working tree equals `ours` when this runs (merges require a clean
/// tree), so only conflicted paths are touched: content/add-add conflicts get
/// `merge3` marker output, modify/delete keeps whichever side still has the
/// file, and binary conflicts keep ours untouched.
fn write_conflict_markers(
    store: &FileObjectStore,
    merge_result: &sorrel_core::MergeResult,
    base_id: &ObjectId,
    ours_id: &ObjectId,
    theirs_id: &ObjectId,
) -> io::Result<Vec<String>> {
    let base_files = to_io(read_snapshot_files(store, base_id))?;
    let ours_files = to_io(read_snapshot_files(store, ours_id))?;
    let theirs_files = to_io(read_snapshot_files(store, theirs_id))?;
    let empty: Vec<u8> = Vec::new();

    let mut paths = Vec::new();
    for conflict_id in &merge_result.conflicts {
        let conflict = to_io(read_conflict(store, conflict_id))?;
        let path = conflict.path.clone();
        paths.push(path.to_string_lossy().into_owned());

        match conflict.conflict_type {
            ConflictType::Content | ConflictType::AddAdd => {
                let base = base_files.get(&path).unwrap_or(&empty);
                let ours = ours_files.get(&path).unwrap_or(&empty);
                let theirs = theirs_files.get(&path).unwrap_or(&empty);
                if let MergeOutcome::Conflicted {
                    merged_with_markers,
                    ..
                } = merge3(base, ours, theirs)
                {
                    write_worktree_file(&path, &merged_with_markers)?;
                }
            }
            ConflictType::ModifyDelete => {
                // Keep the surviving modified version; when ours deleted the
                // file, resurrect theirs so the conflict is visible.
                if !ours_files.contains_key(&path) {
                    if let Some(theirs) = theirs_files.get(&path) {
                        write_worktree_file(&path, theirs)?;
                    }
                }
            }
            ConflictType::Binary => {}
        }
    }

    paths.sort();
    Ok(paths)
}

/// Writes `bytes` to a repo-relative working-tree path, creating parents.
fn write_worktree_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, bytes)
}

/// Returns true when the working tree differs from `base_snapshot`.
fn worktree_is_dirty(
    store: &FileObjectStore,
    repo_id: &str,
    base_snapshot: &ObjectId,
) -> io::Result<bool> {
    let current = materialize_worktree(store, repo_id, None, &[], None)?;
    let diff = to_io(snapshot_diff(store, base_snapshot, &current))?;
    Ok(!diff.is_empty())
}

/// Restores `target` into the working tree, deleting paths that exist in
/// `current` but not in `target`. Never modifies `.sorrel/`.
fn restore_worktree_to_snapshot(
    store: &FileObjectStore,
    current: &ObjectId,
    target: &ObjectId,
) -> io::Result<()> {
    let current_files = to_io(read_snapshot_files(store, current))?;
    let target_files = to_io(read_snapshot_files(store, target))?;

    for path in current_files.keys() {
        if !target_files.contains_key(path) {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            remove_empty_parent_dirs(path)?;
        }
    }

    to_io(restore_snapshot_to_directory(store, target, Path::new(".")))?;
    Ok(())
}

/// Removes empty parent directories of `path` up to (but not including) `.`.
fn remove_empty_parent_dirs(path: &Path) -> io::Result<()> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.as_os_str().is_empty() || dir == Path::new(".") {
            break;
        }
        match fs::remove_dir(dir) {
            Ok(()) => current = dir.parent(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            // Directory not empty or not removable — stop walking up.
            Err(_) => break,
        }
    }
    Ok(())
}

fn slice_create_output(args: SliceCreateArgs) -> io::Result<CommandOutput> {
    let language = if is_js_ts_slice(&args) {
        "javascript/typescript"
    } else {
        "generic"
    };
    let slice = local_slice(&args, language);
    let persisted = workspace_initialized();
    if persisted {
        persist_slice_if_initialized(&args.name, &slice)?;
    }

    Ok(CommandOutput {
        json: json!({
            "command": "slice create",
            "mocked": false,
            "status": "created",
            "persisted": persisted,
            "object": slice
        }),
        human: format!(
            "Created {language} Slice {} ({})",
            args.name,
            if persisted {
                "persisted"
            } else {
                "not persisted: run `sorrel init`"
            }
        ),
    })
}

fn policy_evaluate_output(args: PolicyEvaluateArgs) -> io::Result<CommandOutput> {
    let principal = PrincipalId::parse(&args.principal).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid principal `{}` (expected kind:id)", args.principal),
        )
    })?;
    let resource = ResourceRef::parse(&args.resource).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid resource `{}` (expected scope:id)", args.resource),
        )
    })?;

    let context = PolicyContext::headless_default();
    let core_decision = evaluate(
        &EvaluateInput {
            principal: principal.clone(),
            action: args.action.clone(),
            resource: resource.clone(),
            environment: Some(args.environment.clone()),
        },
        &context,
    );

    let status = core_decision.decision.as_str();
    let decision_json = core_decision_to_json(&core_decision, &args);

    Ok(CommandOutput {
        json: json!({
            "command": "policy evaluate",
            "mocked": false,
            "status": status,
            "decision": decision_json
        }),
        human: format!(
            "Policy decision {} on {}:{}: {}",
            args.action, resource.scope, resource.id, status
        ),
    })
}

fn policy_change_apply_output(args: PolicyChangeApplyArgs) -> io::Result<CommandOutput> {
    let change = load_policy_change(&args)?;
    let context = PolicyContext::headless_default();
    let evaluation = evaluate_policy_change(&change, &context);
    let status = evaluation.decision.as_str();

    Ok(CommandOutput {
        json: json!({
            "command": "policy change apply",
            "mocked": false,
            "status": status,
            "trusted": evaluation.trusted,
            "evaluation": {
                "schemaVersion": PROTOCOL_VERSION,
                "kind": "PolicyChangeEvaluation",
                "actor": principal_json(&evaluation.actor),
                "operation": evaluation.operation,
                "decision": status,
                "reason": evaluation.reason,
                "trusted": evaluation.trusted,
                "evaluatedAt": POLICY_EVALUATED_AT,
                "metadata": {
                    "mocked": false,
                    "backend": "sorrel-core"
                }
            },
            "change": policy_change_to_json(&change)
        }),
        human: format!(
            "Policy change {} by {}: {} ({})",
            change.operation,
            change.actor.to_ref(),
            status,
            if evaluation.trusted {
                "trusted"
            } else {
                "untrusted"
            }
        ),
    })
}

fn load_policy_change(args: &PolicyChangeApplyArgs) -> io::Result<PolicyChange> {
    if let Some(path) = &args.file {
        let bytes = fs::read(path)?;
        return serde_json::from_slice(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid PolicyChange JSON in {}: {error}", path),
            )
        });
    }

    let actor = args.actor.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "policy change apply requires --file or --actor",
        )
    })?;
    let actor = PrincipalId::parse(actor).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid actor `{}` (expected kind:id)", actor),
        )
    })?;
    let target_principal = args.target_principal.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "inline policy change apply requires --target-principal",
        )
    })?;
    let target = PrincipalId::parse(target_principal).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "invalid target principal `{}` (expected kind:id)",
                target_principal
            ),
        )
    })?;

    Ok(PolicyChange {
        actor,
        operation: args.operation.clone(),
        grant: Some(cli_policy::ProposedGrant {
            principal: target,
            capabilities: if args.capabilities.is_empty() {
                vec!["path.write".to_owned()]
            } else {
                args.capabilities.clone()
            },
            resources: vec![cli_policy::ResourceScope {
                scope: "repo".to_owned(),
                fields: json!({ "ref": "repo_mock_local" })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            }],
        }),
        signatures: args.signatures.clone(),
    })
}

fn core_decision_to_json(
    core_decision: &cli_policy::PolicyDecision,
    args: &PolicyEvaluateArgs,
) -> Value {
    let required_grant = if core_decision.decision == Decision::NeedsGrant {
        json!({
            "kind": "Grant",
            "action": &args.action,
            "resource": {
                "type": core_decision.resource.scope,
                "ref": core_decision.resource.id
            },
            "environment": &args.environment
        })
    } else {
        Value::Null
    };

    json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "PolicyDecision",
        "id": format!("decision_{}", args.action.replace('.', "_")),
        "action": &args.action,
        "subject": principal_json(&core_decision.principal),
        "resource": {
            "type": core_decision.resource.scope,
            "ref": core_decision.resource.id
        },
        "environment": &args.environment,
        "result": core_decision.decision.as_str(),
        "effect": core_decision.decision.effect(),
        "policy": {
            "kind": "Policy",
            "id": "policy_headless_core"
        },
        "matchedRule": {
            "effect": core_decision.decision.effect(),
            "action": &args.action,
            "subjects": [
                core_decision.principal.to_ref()
            ],
            "resources": [
                {
                    "type": core_decision.resource.scope,
                    "ref": core_decision.resource.id
                }
            ],
            "reason": core_decision.reason
        },
        "requiredGrant": required_grant,
        "evaluatedAt": POLICY_EVALUATED_AT,
        "metadata": {
            "mocked": false,
            "backend": "sorrel-core"
        }
    })
}

fn policy_change_to_json(change: &PolicyChange) -> Value {
    json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "PolicyChange",
        "actor": principal_json(&change.actor),
        "operation": change.operation,
        "grant": change.grant.as_ref().map(|grant| json!({
            "principal": principal_json(&grant.principal),
            "capabilities": grant.capabilities,
            "resources": grant.resources.iter().map(resource_scope_json).collect::<Vec<_>>()
        })),
        "signatures": change.signatures
    })
}

fn principal_json(principal: &PrincipalId) -> Value {
    json!({
        "type": principal.kind,
        "ref": principal.to_ref()
    })
}

fn resource_scope_json(scope: &cli_policy::ResourceScope) -> Value {
    let mut value = json!({ "scope": scope.scope });
    if let Some(object) = value.as_object_mut() {
        for (key, field) in &scope.fields {
            object.insert(key.clone(), field.clone());
        }
    }
    value
}

fn grant_create_output(args: GrantCreateArgs) -> io::Result<CommandOutput> {
    let (grant_json, human) = grant_create_real(&args)?;
    Ok(CommandOutput {
        json: grant_json,
        human,
    })
}

/// Builds a real Grant document, evaluates the authorizing Core decision, and
/// persists the grant under `.sorrel/grants/` only when the workspace is
/// initialized. The grant is keyed by a content-derived id.
fn grant_create_real(args: &GrantCreateArgs) -> io::Result<(Value, String)> {
    // Evaluate the authorizing decision through Core: does the requesting
    // principal have the action on the target secret resource?
    let resource = ResourceRef::parse(&format!("secret:{}", args.secret)).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid secret ref `{}`", args.secret),
        )
    })?;
    let principal = PrincipalId::parse(&format!("agent:{}", args.agent)).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid agent principal `{}`", args.agent),
        )
    })?;
    let context = PolicyContext::headless_default();
    let decision = evaluate(
        &EvaluateInput {
            principal,
            action: args.action.clone(),
            resource,
            environment: Some(args.environment.clone()),
        },
        &context,
    );
    let status = decision.decision.as_str().to_owned();

    let seed = format!(
        "{}|{}|{}|{}|{}|{}",
        args.action, args.agent, args.workflow, args.runner, args.secret, args.environment
    );
    let grant_id = format!(
        "grant_{}",
        &sorrel_core::ObjectId::for_bytes(seed.as_bytes()).to_hex()[..16]
    );

    let grant = json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "Grant",
        "id": grant_id,
        "action": args.action,
        "resource": { "type": "secret", "ref": args.secret },
        "environment": args.environment,
        "access": {
            "agents": [{ "kind": "AgentPolicy", "id": args.agent }],
            "workflows": [{ "kind": "Workflow", "id": args.workflow }],
            "runners": [{ "kind": "Runner", "id": args.runner }]
        },
        "reason": args.reason,
        "createdAt": repo::now_rfc3339(),
        "decision": status,
        "metadata": { "mocked": false, "backend": "local-headless" }
    });

    let persisted = if repo::is_initialized() {
        repo::write_registry_entry(repo::GRANTS_DIR, &grant_id, &grant)?;
        true
    } else {
        false
    };

    let out = json!({
        "command": "grant create",
        "mocked": false,
        "status": status,
        "persisted": persisted,
        "object": grant
    });
    let human = format!(
        "Grant {grant_id} {} ({}: {status})",
        if persisted {
            "persisted"
        } else {
            "not persisted: run `sorrel init`"
        },
        args.action
    );
    Ok((out, human))
}

fn grant_list_output() -> io::Result<CommandOutput> {
    let objects = repo::list_registry_entries(repo::GRANTS_DIR)?;
    let mut human = String::new();
    for object in &objects {
        let id = object["id"].as_str().unwrap_or_default();
        let action = object["action"].as_str().unwrap_or_default();
        let secret = object["resource"]["ref"].as_str().unwrap_or_default();
        human.push_str(&format!("{id}  {action}  {secret}\n"));
    }
    if objects.is_empty() {
        human = "No grants recorded (use `sorrel grant create`)".to_owned();
    }
    Ok(CommandOutput {
        json: json!({
            "command": "grant list",
            "mocked": false,
            "count": objects.len(),
            "objects": objects
        }),
        human: human.trim_end().to_owned(),
    })
}

fn remote_add_output(args: RemoteAddArgs) -> io::Result<CommandOutput> {
    if !repo::is_initialized() {
        return Err(io::Error::other(
            "workspace is not initialized; run `sorrel init`",
        ));
    }

    let repo_id = match args.repo_id {
        Some(id) => id,
        None => {
            let manifest = repo::load_manifest()?.ok_or_else(|| {
                io::Error::other("workspace is not initialized; run `sorrel init`")
            })?;
            manifest
                .get("repoId")
                .and_then(Value::as_str)
                .ok_or_else(|| io::Error::other("manifest missing repoId"))?
                .to_owned()
        }
    };

    repo::add_remote(&args.name, &args.url, &repo_id)?;

    Ok(CommandOutput {
        json: json!({
            "command": "remote add",
            "mocked": false,
            "status": "added",
            "remote": {
                "name": args.name,
                "url": args.url,
                "repoId": repo_id,
            }
        }),
        human: format!("Added remote {} -> {} ({repo_id})", args.name, args.url),
    })
}

fn remote_list_output() -> io::Result<CommandOutput> {
    let config = repo::load_remotes()?;
    let mut remotes_json = serde_json::Map::new();
    let mut human = String::new();
    for (name, remote) in &config.remotes {
        remotes_json.insert(
            name.clone(),
            json!({
                "url": remote.url,
                "repoId": remote.repo_id,
            }),
        );
        human.push_str(&format!("{name}  {}  {}\n", remote.url, remote.repo_id));
    }
    if config.remotes.is_empty() {
        human = "No remotes configured (use `sorrel remote add`)".to_owned();
    }
    Ok(CommandOutput {
        json: json!({
            "command": "remote list",
            "mocked": false,
            "count": config.remotes.len(),
            "remotes": remotes_json,
        }),
        human: human.trim_end().to_owned(),
    })
}

fn push_output(args: PushArgs) -> io::Result<CommandOutput> {
    let RepoContext { head, store, .. } = open_repo()?;
    let snapshot_id =
        head_snapshot_id(&head)?.ok_or_else(|| io::Error::other("HEAD has no snapshot to push"))?;

    let remotes = repo::load_remotes()?;
    let (remote_name, remote) = remotes.resolve(args.remote.as_deref())?;
    let result = sync::push(
        &store,
        &remote,
        &remote_name,
        &args.r#ref,
        &snapshot_id,
        None,
    )?;

    Ok(CommandOutput {
        json: json!({
            "command": "push",
            "mocked": false,
            "status": "pushed",
            "remote": result.remote,
            "ref": result.ref_name,
            "snapshot": { "kind": "Snapshot", "id": result.snapshot },
            "uploaded": result.uploaded,
        }),
        human: format!(
            "Pushed {} to {}/{} ({} object(s))",
            &result.snapshot[..result.snapshot.len().min(12)],
            result.remote,
            result.ref_name,
            result.uploaded
        ),
    })
}

fn pull_output(args: PullArgs) -> io::Result<CommandOutput> {
    let store = to_io(FileObjectStore::new(repo::object_store_root()))?;
    if !repo::is_initialized() {
        return Err(io::Error::other(
            "workspace is not initialized; run `sorrel init`",
        ));
    }

    let remotes = repo::load_remotes()?;
    let (remote_name, remote) = remotes.resolve(args.remote.as_deref())?;
    let before = repo::load_head()?.ok_or_else(|| io::Error::other("missing HEAD pointer"))?;
    let before_snapshot = parse_object_id_hex(&before.snapshot)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let result = sync::pull(&store, &remote, &remote_name, &args.r#ref, None)?;
    let after_snapshot = parse_object_id_hex(&result.snapshot)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    if before_snapshot != after_snapshot {
        restore_worktree_to_snapshot(&store, &before_snapshot, &after_snapshot)?;
    }

    Ok(CommandOutput {
        json: json!({
            "command": "pull",
            "mocked": false,
            "status": "pulled",
            "remote": result.remote,
            "ref": result.ref_name,
            "snapshot": { "kind": "Snapshot", "id": result.snapshot },
            "downloaded": result.downloaded,
        }),
        human: format!(
            "Pulled {}/{} to {} ({} object(s))",
            result.remote,
            result.ref_name,
            &result.snapshot[..result.snapshot.len().min(12)],
            result.downloaded
        ),
    })
}

fn local_slice(args: &SliceCreateArgs, language: &str) -> Value {
    // Derive a stable slice id from its defining fields.
    let seed = format!(
        "{}|{}|{}|{}",
        args.name, args.source_repo, args.source_path, args.entrypoint
    );
    let slice_id = format!(
        "slice_{}",
        &sorrel_core::ObjectId::for_bytes(seed.as_bytes()).to_hex()[..16]
    );
    json!({
        "schemaVersion": PROTOCOL_VERSION,
        "kind": "Slice",
        "id": slice_id,
        "name": &args.name,
        "sourceRepo": &args.source_repo,
        "sourcePath": &args.source_path,
        "entrypoints": [
            &args.entrypoint
        ],
        "permissionsPolicy": "projected",
        "linkMode": "live",
        "includes": [
            {
                "type": "path",
                "value": &args.source_path
            }
        ],
        "excludes": [],
        "createdBy": {
            "type": "agent",
            "id": "agent_local_cli",
            "displayName": "Sorrel CLI"
        },
        "createdAt": repo::now_rfc3339(),
        "metadata": {
            "mocked": false,
            "language": language,
            "backend": "local"
        }
    })
}

fn write_json(mut writer: impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut writer, value)?;
    writeln!(writer)
}

fn write_json_file(path: &Path, value: &Value) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    write_json(&mut file, value)
}

/// Maps an engine error into an `io::Error` for uniform CLI error reporting.
fn to_io<T, E: std::fmt::Display>(result: Result<T, E>) -> io::Result<T> {
    result.map_err(|error| io::Error::other(error.to_string()))
}

/// Parses the persisted HEAD snapshot id, if any, into an engine `ObjectId`.
fn head_snapshot_id(head: &repo::Head) -> io::Result<Option<ObjectId>> {
    if head.snapshot.is_empty() {
        return Ok(None);
    }
    let id = head
        .snapshot
        .parse::<ObjectId>()
        .map_err(|error| io::Error::other(format!("invalid HEAD snapshot id: {error}")))?;
    Ok(Some(id))
}

/// Materializes the current working tree (excluding `.sorrel/`) into the object
/// store and returns the resulting snapshot id (hex) plus its `ObjectId`.
///
/// When `stat_cache` is `Some`, unchanged files (same size + mtime with a live
/// blob in the store) are not re-hashed; the cache is updated in place and the
/// caller is responsible for persisting it after a successful command.
fn materialize_worktree(
    store: &FileObjectStore,
    repo_id: &str,
    message: Option<String>,
    parents: &[ObjectId],
    stat_cache: Option<&mut StatCache>,
) -> io::Result<ObjectId> {
    let mut options = SnapshotOptions::new(repo_id.to_owned());
    options.created_at = repo::now_rfc3339();
    options.message = message;
    options.parents = parents
        .iter()
        .map(|id| ObjectRef::new(ObjectKind::Snapshot, *id))
        .collect();
    // Snapshot the working tree in place, excluding the on-disk object store
    // (`.sorrel/`) and a colocated Git metadata dir (`.git/`) at the root.
    // No copy-to-scratch. The stat cache lets unchanged files skip re-hashing.
    let snapshot = to_io(materialize_snapshot_excluding_with_stat_cache(
        store,
        Path::new("."),
        [repo::SORREL_DIR, ".git"],
        stat_cache,
        options,
    ))?;
    Ok(snapshot.id)
}

/// Renders a `SnapshotDiff` into grouped added/modified/deleted path lists.
fn diff_json(diff: &sorrel_core::SnapshotDiff) -> (Value, usize) {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    for change in &diff.changes {
        let path = change.path.to_string_lossy().into_owned();
        match change.kind {
            PathChangeKind::Added => added.push(path),
            PathChangeKind::Modified => modified.push(path),
            PathChangeKind::Deleted => deleted.push(path),
        }
    }
    let total = added.len() + modified.len() + deleted.len();
    (
        json!({ "added": added, "modified": modified, "deleted": deleted }),
        total,
    )
}

fn workspace_initialized() -> bool {
    repo::is_initialized()
}

fn is_js_ts_slice(args: &SliceCreateArgs) -> bool {
    let source_path = Path::new(&args.source_path);
    let entrypoint = Path::new(&args.entrypoint);

    has_js_ts_extension(source_path)
        || has_js_ts_extension(entrypoint)
        || source_path.join("package.json").is_file()
        || source_path.join("tsconfig.json").is_file()
        || Path::new("package.json").is_file()
        || Path::new("tsconfig.json").is_file()
}

fn has_js_ts_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
    )
}

fn persist_slice_if_initialized(name: &str, slice: &Value) -> io::Result<()> {
    if !workspace_initialized() {
        return Ok(());
    }

    let slices_dir = repo::sorrel_dir().join(repo::SLICES_DIR);
    fs::create_dir_all(&slices_dir)?;
    write_json_file(&slices_dir.join(slice_file_name(name)), slice)
}

fn slice_file_name(name: &str) -> PathBuf {
    let safe_name: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();

    PathBuf::from(format!("{safe_name}.json"))
}
