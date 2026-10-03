use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn node(root: &Path, script: &str) -> Value {
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sorrel-agents/src/index.js");
    let script = format!(
        "import {{pathToFileURL}} from 'node:url';\nconst {{AgentControlPlane}} = await import(pathToFileURL({}));\nconst plane = new AgentControlPlane({{workspace:process.cwd()}});\n{script}",
        serde_json::to_string(&module.to_string_lossy()).unwrap()
    );
    let output = ProcessCommand::new("node")
        .current_dir(root)
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("Node 20+ is required for the real registry interoperability test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_and_node_share_registration_claims_overlaps_and_releases() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let registered = run(
        root,
        &["agent", "register", "cli_agent", "--task", "Parser"],
    );
    assert_eq!(registered["agent"]["task"], "Parser");
    run(root, &["agent", "claim", "cli_agent", "./src//lib/"]);
    let js = node(root, "await plane.registerAgent({id:'node_agent'}); await plane.claimPath({agentId:'node_agent',path:'src/lib/file.rs'}); console.log(JSON.stringify(await plane.activeWork()));");
    assert_eq!(js["agents"].as_array().unwrap().len(), 2);
    assert_eq!(
        js["overlaps"],
        json!([{"path":"src/lib","agentIds":["cli_agent","node_agent"]}])
    );
    let cli = run(root, &["agent", "active"]);
    assert_eq!(cli["claims"], js["claims"]);
    assert_eq!(cli["overlaps"], js["overlaps"]);
    assert_eq!(cli["agents"], js["agents"]);
    let released = run(
        root,
        &["agent", "release", "node_agent", "./src/lib/file.rs"],
    );
    assert_eq!(released["released"], true);
    assert_eq!(
        run(root, &["agent", "release", "node_agent", "src/lib/file.rs"])["released"],
        false
    );
    let js = node(root, "await plane.releasePath({agentId:'cli_agent',path:'src\\\\lib'}); console.log(JSON.stringify(await plane.activeWork()));");
    assert_eq!(js["claims"], json!([]));
    assert_eq!(run(root, &["agent", "active"])["claims"], json!([]));
}

#[test]
fn legacy_migration_is_shared_and_preserves_existing_records() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["agent", "register", "existing", "--display-name", "Current"],
    );
    let state_dir = root.join(".sorrel/agents");
    let legacy = json!({
        "agents":[{"id":"existing","lane":"lane_old","displayName":"Old","registeredAt":"2026-01-01T00:00:00.000Z"}],
        "claims":[{"agentId":"existing","path":"./src//file","mode":"advisory","claimedAt":"2026-01-01T00:00:00.000Z"}]
    });
    fs::write(
        state_dir.join("state.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let cli = run(root, &["agent", "active"]);
    assert_eq!(cli["agents"][0]["displayName"], "Current");
    assert_eq!(cli["claims"][0]["path"], "src/file");
    let backup: Value =
        serde_json::from_slice(&fs::read(state_dir.join("state.migrated.json")).unwrap()).unwrap();
    assert_eq!(backup, legacy);
    assert_eq!(cli["workspaces"], json!([]));
    let mut cli_registry = cli;
    cli_registry.as_object_mut().unwrap().remove("workspaces");
    assert_eq!(
        node(
            root,
            "console.log(JSON.stringify(await plane.activeWork()));"
        ),
        cli_registry
    );
    run(root, &["agent", "release", "existing", "src/file"]);
    assert_eq!(run(root, &["agent", "active"])["claims"], json!([]));
}

#[test]
fn unsafe_inputs_and_corrupt_registry_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    for args in [
        vec!["agent", "register", "../escape"],
        vec!["agent", "claim", "missing", "src"],
    ] {
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(args)
            .assert()
            .failure();
    }
    run(root, &["agent", "register", "safe"]);
    for path in ["/etc/passwd", "../escape", "src/../file", "C:\\escape", "."] {
        Command::cargo_bin("sorrel")
            .unwrap()
            .current_dir(root)
            .args(["agent", "claim", "safe", path])
            .assert()
            .failure();
    }
    fs::write(root.join(".sorrel/agents/agents/safe.json"), "{").unwrap();
    Command::cargo_bin("sorrel")
        .unwrap()
        .current_dir(root)
        .args(["agent", "active"])
        .assert()
        .failure();
    assert!(!root.join(".sorrel/agents/agents/escape.json").exists());
}

#[test]
fn independent_cli_processes_preserve_all_agent_records() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let binary = assert_cmd::cargo::cargo_bin("sorrel");
    let mut children: Vec<_> = (0..6)
        .map(|index| {
            ProcessCommand::new(&binary)
                .current_dir(root)
                .args(["agent", "register", &format!("agent_{index}"), "--json"])
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(
        run(root, &["agent", "active"])["agents"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn active_work_includes_live_worker_readiness_without_changing_registry_contract() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("owner");
    let worker = dir.path().join("worker");
    fs::create_dir(&root).unwrap();
    run(&root, &["init"]);
    run(
        &root,
        &[
            "workspace",
            "create",
            worker.to_str().unwrap(),
            "--agent",
            "worker",
        ],
    );
    fs::write(worker.join("work.txt"), "recorded\n").unwrap();
    run(&worker, &["change", "create", "-m", "Ready work"]);
    let active = run(&root, &["agent", "active"]);
    assert_eq!(active["agents"][0]["id"], "worker");
    assert_eq!(active["workspaces"][0]["status"], "ready");
    assert_eq!(active["workspaces"][0]["pendingSnapshots"], 1);
    assert_eq!(active["workspaces"][0]["readyToIntegrate"], true);
    fs::write(worker.join("work.txt"), "unrecorded\n").unwrap();
    let dirty = run(&root, &["agent", "active"]);
    assert_eq!(dirty["workspaces"][0]["status"], "dirty");
    assert_eq!(dirty["workspaces"][0]["readyToIntegrate"], false);
    assert_eq!(
        dirty["workspaces"][0]["blockers"],
        json!(["dirty_worktree"])
    );
}
