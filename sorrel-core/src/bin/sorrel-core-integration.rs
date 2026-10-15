//! Bounded native proposal integration and filesystem writer-lease bridge.
#[path = "support/policy_wire.rs"]
mod policy_wire;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sorrel_core::{
    merge_bases, merge_snapshots, read_blob, read_snapshot, read_tree, EntryMode, EntryType,
    MergeError, MergeOptions, ObjectId, ObjectKind, ObjectStore, ObjectStoreError,
    ObjectStoreResult, Principal,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::PathBuf,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_WIRE: usize = 2 * 1024 * 1024;
const MAX_OBJECT: usize = 1024 * 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    #[serde(rename = "type")]
    kind: String,
    id: String,
}
impl Identity {
    fn valid(&self) -> bool {
        [
            "user", "agent", "team", "service", "runner", "system", "workflow",
        ]
        .contains(&self.kind.as_str())
            && !self.id.trim().is_empty()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Inputs {
    proposal_id: String,
    project_id: String,
    repository_id: Option<String>,
    repo_id: String,
    source_ref: String,
    target_ref: String,
    source_snapshot: String,
    target_snapshot: String,
    source_generation: String,
    target_generation: String,
    author: Identity,
    review: Value,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequiredCheck {
    id: String,
    reporter: Identity,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Requirements {
    version: u64,
    checks: Vec<RequiredCheck>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    version: u8,
    id: String,
    nonce: String,
    inputs: Inputs,
    review_revision: String,
    requirements_digest: String,
    merge_base: Option<String>,
    candidate_snapshot: Option<String>,
    merge_result_id: Option<String>,
    conflicts: Vec<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Approval {
    binding_id: String,
    candidate_snapshot: String,
    principal: Identity,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckReport {
    binding_id: String,
    candidate_snapshot: String,
    source_snapshot: String,
    target_snapshot: String,
    requirements_digest: String,
    check_id: String,
    attempt: u64,
    run_id: String,
    outcome: String,
    principal: Identity,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    operation: String,
    root: PathBuf,
    inputs: Inputs,
    requirements: Option<Requirements>,
    principal: Identity,
    #[serde(default)]
    grants: Vec<Value>,
    #[serde(default)]
    policies: Vec<Value>,
    nonce: Option<String>,
    binding: Option<Binding>,
    #[serde(default)]
    approvals: Vec<Approval>,
    #[serde(default)]
    checks: Vec<CheckReport>,
    report: Option<CheckReport>,
}
fn digest(value: &impl Serialize) -> Result<String> {
    // serde_json's default ordered maps give a canonical recursive JSON encoding.
    Ok(ObjectId::for_bytes(&serde_json::to_vec(&serde_json::to_value(value)?)?).to_string())
}
fn oid(value: &str) -> Result<ObjectId> {
    let id: ObjectId = value.parse()?;
    if id.to_string() != value {
        return Err("noncanonical object id".into());
    }
    Ok(id)
}
fn ref_valid(name: &str) -> bool {
    !name.is_empty()
        && name.split('/').all(|segment| {
            segment
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
                && segment
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}

/// Digest-verifying Hub-layout adapter. Every read and write consumes the budget,
/// including repeated engine reads; no unbounded blob read precedes validation.
struct HubObjects {
    root: PathBuf,
    bytes: Cell<usize>,
    writes: RefCell<BTreeMap<ObjectId, Vec<u8>>>,
}
impl HubObjects {
    fn path(&self, id: &ObjectId) -> PathBuf {
        let hex = id.to_string();
        self.root.join("objects").join(&hex[..2]).join(hex)
    }
    fn budget(&self, len: usize) -> io::Result<()> {
        let next = self.bytes.get().saturating_add(len);
        if len > MAX_OBJECT || next > MAX_BYTES {
            return Err(io::Error::other("integration object byte limit"));
        }
        self.bytes.set(next);
        Ok(())
    }
    fn commit(&self) -> Result<()> {
        for (id, bytes) in self.writes.borrow().iter() {
            let path = self.path(id);
            fs::create_dir_all(path.parent().unwrap())?;
            let temp = path.with_extension(format!("{}.tmp", std::process::id()));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            let result = (|| -> Result<()> {
                file.write_all(bytes)?;
                file.sync_all()?;
                fs::rename(&temp, &path)?;
                sorrel_core::durability::flush_directory_chain(path.parent().unwrap(), &self.root)?;
                Ok(())
            })();
            if temp.exists() {
                fs::remove_file(&temp)?;
            }
            result?;
        }
        Ok(())
    }
}
impl ObjectStore for HubObjects {
    fn read(&self, id: &ObjectId) -> ObjectStoreResult<Vec<u8>> {
        let path = self.path(id);
        let io_error = |source| ObjectStoreError::Io {
            path: path.clone(),
            source,
        };
        let bytes = if let Some(bytes) = self.writes.borrow().get(id) {
            self.budget(bytes.len()).map_err(io_error)?;
            bytes.clone()
        } else {
            let mut file = File::open(&path).map_err(io_error)?;
            self.budget(
                file.metadata()
                    .map_err(io_error)?
                    .len()
                    .try_into()
                    .unwrap_or(usize::MAX),
            )
            .map_err(io_error)?;
            let mut bytes = Vec::new();
            (&mut file)
                .take((MAX_OBJECT + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            if bytes.len() > MAX_OBJECT {
                return Err(io_error(io::Error::other("object too large")));
            }
            bytes
        };
        let actual = ObjectId::for_bytes(&bytes);
        if actual != *id {
            return Err(ObjectStoreError::ContentMismatch {
                expected: *id,
                actual,
            });
        }
        Ok(bytes)
    }
    fn write(&self, bytes: &[u8]) -> ObjectStoreResult<ObjectId> {
        self.budget(bytes.len())
            .map_err(|source| ObjectStoreError::Io {
                path: self.root.clone(),
                source,
            })?;
        let id = ObjectId::for_bytes(bytes);
        if self.writes.borrow().len() >= MAX_NODES {
            return Err(ObjectStoreError::Io {
                path: self.root.clone(),
                source: io::Error::other("output object limit"),
            });
        }
        self.writes.borrow_mut().insert(id, bytes.to_vec());
        Ok(id)
    }
    fn has(&self, id: &ObjectId) -> ObjectStoreResult<bool> {
        Ok(self.path(id).is_file() || self.writes.borrow().contains_key(id))
    }
}

fn validate_closure(
    store: &HubObjects,
    inputs: &Inputs,
    roots: &[ObjectId],
) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut snapshots = roots.to_vec();
    let mut edges = 0;
    while let Some(id) = snapshots.pop() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > MAX_NODES {
            return Err("snapshot graph limit".into());
        }
        let snapshot = read_snapshot(store, &id)?;
        if snapshot.repo != inputs.repo_id
            || snapshot.root_tree.kind != ObjectKind::Tree
            || snapshot
                .parents
                .iter()
                .any(|p| p.kind != ObjectKind::Snapshot)
        {
            return Err("snapshot repository or reference kind mismatch".into());
        }
        snapshots.extend(snapshot.parents.iter().map(|p| p.id));
        let mut trees = vec![(snapshot.root_tree.id, PathBuf::new(), 0)];
        while let Some((tree_id, parent, depth)) = trees.pop() {
            if depth > 64 {
                return Err("tree depth limit".into());
            }
            seen.insert(tree_id);
            let tree = read_tree(store, &tree_id)?;
            let mut names = BTreeSet::new();
            for entry in tree.entries {
                edges += 1;
                if edges > MAX_NODES * 4 || seen.len() > MAX_NODES {
                    return Err("object graph limit".into());
                }
                if entry.name.is_empty()
                    || entry.name == "."
                    || entry.name == ".."
                    || entry.name.contains(['/', '\\'])
                    || !names.insert(entry.name.clone())
                    || entry.path != parent.join(&entry.name)
                {
                    return Err("noncanonical tree path".into());
                }
                seen.insert(entry.object.id);
                match entry.entry_type {
                    EntryType::Directory
                        if entry.object.kind == ObjectKind::Tree
                            && entry.mode == EntryMode::Directory =>
                    {
                        trees.push((entry.object.id, entry.path, depth + 1))
                    }
                    EntryType::File
                        if entry.object.kind == ObjectKind::Blob
                            && entry.mode != EntryMode::Directory =>
                    {
                        let blob = read_blob(store, &entry.object.id)?;
                        if blob.content.iter().filter(|b| **b == b'\n').count() > 2000 {
                            return Err("text merge line limit".into());
                        }
                        if entry.size.is_some_and(|size| size != blob.size())
                            || entry
                                .content_hash
                                .is_some_and(|hash| hash != blob.content_hash)
                        {
                            return Err("blob metadata mismatch".into());
                        }
                    }
                    _ => return Err("tree entry reference kind/mode mismatch".into()),
                }
            }
        }
    }
    Ok(seen.into_iter().map(|id| id.to_string()).collect())
}
fn allowed(
    request: &Request,
    principal: &Identity,
    action: &str,
    kind: &str,
    id: &str,
) -> Result<bool> {
    if !principal.valid() {
        return Err("invalid authenticated identity".into());
    }
    let wire = serde_json::from_value(json!({"principal": principal, "action": action,
        "resource": {"kind": kind, "id": id}, "grants": request.grants, "policies": request.policies}))?;
    let result = policy_wire::evaluate(wire, jiff::Timestamp::now()).map_err(io::Error::other)?;
    Ok(result["allowed"] == true)
}
fn review_access(r: &Request, principal: &Identity) -> Result<bool> {
    Ok(allowed(
        r,
        principal,
        "proposal.read",
        "project",
        &r.inputs.project_id,
    )? && allowed(
        r,
        principal,
        "proposal.review",
        "project",
        &r.inputs.project_id,
    )? && allowed(r, principal, "repo.read", "repo", &r.inputs.repo_id)?)
}
fn integration_access(r: &Request) -> Result<bool> {
    if !review_access(r, &r.principal)? {
        return Ok(false);
    }
    for action in ["repo.object.write", "repo.ref.write", "proposal.integrate"] {
        if !allowed(r, &r.principal, action, "repo", &r.inputs.repo_id)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn valid_requirements(r: &Option<Requirements>) -> bool {
    r.as_ref().is_some_and(|r| {
        r.version > 0 && !r.checks.is_empty() && r.checks.len() <= 32 && {
            let mut names = BTreeSet::new();
            r.checks.iter().all(|c| {
                !c.id.trim().is_empty()
                    && c.id.len() <= 128
                    && c.reporter.valid()
                    && names.insert(&c.id)
            })
        }
    })
}
fn prepare(r: &Request, store: &HubObjects) -> Result<Binding> {
    let i = &r.inputs;
    if !i.author.valid()
        || i.proposal_id.is_empty()
        || i.project_id.is_empty()
        || !ref_valid(&i.source_ref)
        || !ref_valid(&i.target_ref)
        || i.source_ref == i.target_ref
    {
        return Err("invalid proposal integration inputs".into());
    }
    let source = oid(&i.source_snapshot)?;
    let target = oid(&i.target_snapshot)?;
    validate_closure(store, i, &[source, target])?;
    let bases = merge_bases(store, target, source)?;
    let mut b = Binding {
        version: 1,
        id: String::new(),
        nonce: r.nonce.clone().ok_or("nonce required")?,
        inputs: i.clone(),
        review_revision: digest(&i.review)?,
        requirements_digest: digest(&r.requirements)?,
        merge_base: None,
        candidate_snapshot: None,
        merge_result_id: None,
        conflicts: Vec::new(),
    };
    if b.nonce.len() < 16 || b.nonce.len() > 128 {
        return Err("invalid binding nonce".into());
    }
    if bases.len() != 1 {
        b.conflicts.push(json!({"type":"merge_base", "path":null, "bases":bases.iter().map(ToString::to_string).collect::<Vec<_>>()}));
    } else {
        b.merge_base = Some(bases[0].to_string());
        let options =
            MergeOptions::new(Principal::system(), &i.repo_id, "Hub proposal integration");
        match merge_snapshots(store, &bases[0], &target, &source, &options) {
            Ok(merged) => {
                b.merge_result_id = Some(merged.id.to_string());
                b.candidate_snapshot = merged.merged_snapshot.map(|id| id.to_string());
                for id in merged.conflicts {
                    b.conflicts.push(serde_json::from_slice(&store.read(&id)?)?);
                    b.conflicts.last_mut().unwrap()["id"] = json!(id.to_string());
                }
            }
            Err(MergeError::PathCollision { path }) => b
                .conflicts
                .push(json!({"type":"file_directory", "path":path})),
            Err(error) => return Err(error.into()),
        }
    }
    b.id = digest(&b)?;
    Ok(b)
}
fn report_valid(r: &Request, b: &Binding, c: &CheckReport) -> Result<bool> {
    let required = r
        .requirements
        .as_ref()
        .and_then(|req| req.checks.iter().find(|x| x.id == c.check_id));
    Ok(c.binding_id == b.id
        && Some(&c.candidate_snapshot) == b.candidate_snapshot.as_ref()
        && c.source_snapshot == b.inputs.source_snapshot
        && c.target_snapshot == b.inputs.target_snapshot
        && c.requirements_digest == b.requirements_digest
        && c.attempt > 0
        && c.attempt <= 9_007_199_254_740_991
        && !c.run_id.trim().is_empty()
        && c.run_id.len() <= 256
        && ["pending", "succeeded", "failed", "cancelled"].contains(&c.outcome.as_str())
        && required.is_some_and(|req| req.reporter == c.principal)
        && allowed(
            r,
            &c.principal,
            "proposal.read",
            "project",
            &b.inputs.project_id,
        )?
        && allowed(r, &c.principal, "repo.read", "repo", &b.inputs.repo_id)?
        && allowed(
            r,
            &c.principal,
            "proposal.check.report",
            "repo",
            &b.inputs.repo_id,
        )?)
}
fn execute(r: Request) -> Result<Value> {
    let store = HubObjects {
        root: r.root.clone(),
        bytes: Cell::new(0),
        writes: RefCell::new(BTreeMap::new()),
    };
    if r.operation == "prepare" {
        if !integration_access(&r)? {
            return Err("Core denied integration preparation".into());
        }
        let b = prepare(&r, &store)?;
        store.commit()?;
        return Ok(json!({"binding": b}));
    }
    let b = r.binding.as_ref().ok_or("binding required")?;
    let mut blockers = Vec::new();
    let mut unsigned = b.clone();
    unsigned.id.clear();
    if b.version != 1 || digest(&unsigned)? != b.id {
        return Err("invalid binding identity".into());
    }
    if b.inputs.source_snapshot != r.inputs.source_snapshot
        || b.inputs.source_generation != r.inputs.source_generation
    {
        blockers.push("superseded_source");
    }
    if b.inputs.target_snapshot != r.inputs.target_snapshot
        || b.inputs.target_generation != r.inputs.target_generation
    {
        blockers.push("stale_target");
    }
    if b.inputs != r.inputs || b.review_revision != digest(&r.inputs.review)? {
        blockers.push("review_changed");
    }
    if b.requirements_digest != digest(&r.requirements)? {
        blockers.push("requirements_changed");
    }
    if !valid_requirements(&r.requirements) {
        blockers.push("trusted_required_checks_not_configured");
    }
    if b.candidate_snapshot.is_none() || !b.conflicts.is_empty() {
        blockers.push("conflicted");
    }
    let can_approve = r.principal != r.inputs.author && review_access(&r, &r.principal)?;
    let can_integrate = integration_access(&r)?;
    if !can_integrate {
        blockers.push("integration_authority_denied");
    }
    let mut approved = false;
    for a in &r.approvals {
        if a.binding_id == b.id
            && Some(&a.candidate_snapshot) == b.candidate_snapshot.as_ref()
            && a.principal != r.inputs.author
            && review_access(&r, &a.principal)?
        {
            approved = true;
        }
    }
    if !approved {
        blockers.push("non_author_approval_required");
    }
    let mut check_states = Vec::new();
    if let Some(req) = &r.requirements {
        for check in &req.checks {
            let latest = r
                .checks
                .iter()
                .filter(|c| c.check_id == check.id && c.binding_id == b.id)
                .max_by_key(|c| c.attempt);
            let success = if let Some(c) = latest {
                c.outcome == "succeeded" && report_valid(&r, b, c)?
            } else {
                false
            };
            if !success {
                blockers.push("required_check_missing_or_unsuccessful");
            }
            check_states.push(json!({"id":check.id, "reporter":check.reporter, "outcome":latest.map(|c| c.outcome.as_str()).unwrap_or("missing"), "trusted":success, "attempt":latest.map(|c|c.attempt), "runId":latest.map(|c|&c.run_id)}));
        }
    }
    match r.operation.as_str() {
        "readiness" => Ok(
            json!({"blockers":blockers,"canApprove":can_approve,"canIntegrate":can_integrate,"approved":approved,"requiredChecks":check_states,"ready":blockers.is_empty()}),
        ),
        "approve" => {
            if !can_approve
                || blockers.iter().any(|b| {
                    [
                        "superseded_source",
                        "stale_target",
                        "review_changed",
                        "requirements_changed",
                        "conflicted",
                    ]
                    .contains(b)
                })
            {
                return Err("approval denied or binding is stale/conflicted".into());
            }
            Ok(
                json!({"approval":Approval { binding_id:b.id.clone(), candidate_snapshot:b.candidate_snapshot.clone().unwrap(), principal:r.principal.clone() }}),
            )
        }
        "report" => {
            let c = r.report.as_ref().ok_or("check report required")?;
            if !valid_requirements(&r.requirements)
                || c.principal != r.principal
                || !report_valid(&r, b, c)?
                || blockers.iter().any(|b| {
                    [
                        "superseded_source",
                        "stale_target",
                        "review_changed",
                        "requirements_changed",
                        "conflicted",
                    ]
                    .contains(b)
                })
            {
                return Err("untrusted or stale check report".into());
            }
            if let Some(old) = r
                .checks
                .iter()
                .filter(|old| old.binding_id == c.binding_id && old.check_id == c.check_id)
                .max_by_key(|old| old.attempt)
            {
                if c.attempt < old.attempt
                    || c.attempt == old.attempt
                        && c != old
                        && !(old.outcome == "pending"
                            && old.run_id == c.run_id
                            && old.principal == c.principal)
                {
                    return Err("check attempt is superseded or terminal".into());
                }
                if c.attempt > old.attempt
                    && r.checks.iter().any(|old| {
                        old.binding_id == c.binding_id
                            && old.check_id == c.check_id
                            && c.run_id == old.run_id
                    })
                {
                    return Err("new attempt requires a new run identity".into());
                }
            }
            Ok(json!({"report":c}))
        }
        "integrate" => {
            if !blockers.is_empty() {
                return Ok(json!({"ready":false,"blockers":blockers}));
            }
            let recompute = Request {
                operation: "prepare".into(),
                root: r.root.clone(),
                inputs: r.inputs.clone(),
                requirements: r.requirements.clone(),
                principal: r.principal.clone(),
                grants: r.grants.clone(),
                policies: r.policies.clone(),
                nonce: Some(b.nonce.clone()),
                binding: None,
                approvals: Vec::new(),
                checks: Vec::new(),
                report: None,
            };
            if prepare(&recompute, &store)? != *b {
                return Err("candidate identity mismatch".into());
            }
            let closure = validate_closure(
                &store,
                &r.inputs,
                &[oid(b.candidate_snapshot.as_ref().unwrap())?],
            )?;
            store.commit()?;
            // Recheck evidence authority after the potentially expensive merge/closure work.
            let mut recheck = r.clone();
            recheck.operation = "readiness".into();
            let now = jiff::Timestamp::now();
            let eligibility = execute(recheck)?;
            if eligibility["ready"] != true {
                return Ok(eligibility);
            }
            let valid_until = r
                .grants
                .iter()
                .flat_map(|grant| {
                    ["issuedAt", "expiresAt"]
                        .into_iter()
                        .filter_map(move |field| grant.get(field).and_then(Value::as_str))
                })
                .filter_map(|value| value.parse::<jiff::Timestamp>().ok())
                .filter(|at| *at > now)
                .min()
                .map(|at| at.to_string());
            Ok(
                json!({"ready":true,"candidateSnapshot":b.candidate_snapshot,"closure":closure,"authorityValidUntil":valid_until}),
            )
        }
        _ => Err("unknown integration operation".into()),
    }
}
fn lock_roots(roots: impl Iterator<Item = String>) -> Result<()> {
    let mut roots = roots
        .map(|root| {
            fs::create_dir_all(&root)?;
            fs::canonicalize(root)
        })
        .collect::<io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        return Err("dataset roots required".into());
    }
    let mut files = Vec::new();
    for root in roots {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".sorrel-hub-writer.lock"))?;
        file.try_lock()
            .map_err(|_| "filesystem dataset already has a Hub writer")?;
        files.push(file);
    }
    println!("{{\"locked\":true}}");
    io::stdout().flush()?;
    // The parent owns stdin. EOF, close, parent death and unwinding drop every OS lock.
    io::copy(&mut io::stdin().take(1), &mut io::sink())?;
    drop(files);
    Ok(())
}
fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("lock") {
        if lock_roots(args).is_err() {
            eprintln!("filesystem dataset writer lease unavailable");
            std::process::exit(1);
        }
        return;
    }
    let result = (|| -> Result<Value> {
        let mut bytes = Vec::new();
        io::stdin()
            .take((MAX_WIRE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_WIRE {
            return Err("integration request byte limit".into());
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if value == json!({"operation":"probe"}) {
            return Ok(json!({"version":1}));
        }
        execute(serde_json::from_value(value)?)
    })();
    let (value, code) = match result {
        Ok(value) => (value, 0),
        Err(error) => (
            json!({"error":{"code":"integration_blocked","message":error.to_string()}}),
            1,
        ),
    };
    let bytes = serde_json::to_vec(&value).expect("response serializes");
    if bytes.len() > MAX_WIRE {
        eprintln!("integration response limit");
        std::process::exit(1);
    }
    if io::stdout().write_all(&bytes).is_err() {
        std::process::exit(1);
    }
    std::process::exit(code);
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use sorrel_core::{
        write_blob, write_snapshot, write_tree, ObjectRef, SnapshotOptions, TreeEntry,
    };
    fn objects(root: &std::path::Path) -> HubObjects {
        HubObjects {
            root: root.to_path_buf(),
            bytes: Cell::new(0),
            writes: RefCell::new(BTreeMap::new()),
        }
    }
    fn request(root: &PathBuf, store: &HubObjects) -> Request {
        let tree = write_tree(store, Vec::new()).unwrap();
        let base = write_snapshot(store, tree.id, SnapshotOptions::new("repo_test")).unwrap();
        let mut options = SnapshotOptions::new("repo_test");
        options.parents = vec![ObjectRef::new(ObjectKind::Snapshot, base.id)];
        let source = write_snapshot(store, tree.id, options).unwrap();
        serde_json::from_value(json!({"operation":"prepare", "root":root,
            "inputs":{"proposalId":"p","projectId":"project","repositoryId":null,"repoId":"repo_test", "sourceRef":"source","targetRef":"main", "sourceSnapshot":source.id.to_string(),"targetSnapshot":base.id.to_string(),"sourceGeneration":"one","targetGeneration":"two","author":{"type":"user","id":"author"},"review":{"metadata":{"custom":[1,2]}}},
            "requirements":{"version":1,"checks":[{"id":"test","reporter":{"type":"service","id":"ci"}}]},"principal":{"type":"user","id":"author"},"nonce":"0123456789abcdef"})).unwrap()
    }
    #[test]
    fn candidate_identity_is_deterministic_and_canonical_review_covers_nested_metadata() {
        let root = tempfile::tempdir().unwrap();
        let store = objects(root.path());
        let mut r = request(&store.root, &store);
        let first = prepare(&r, &store).unwrap();
        assert_eq!(first.id, prepare(&r, &store).unwrap().id);
        r.inputs.review["metadata"]["custom"][0] = json!(9);
        assert_ne!(first.id, prepare(&r, &store).unwrap().id);
        let snapshot = read_snapshot(
            &store,
            &oid(first.candidate_snapshot.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            snapshot
                .parents
                .iter()
                .map(|p| p.id.to_string())
                .collect::<Vec<_>>(),
            [r.inputs.target_snapshot, r.inputs.source_snapshot]
        );
    }
    #[test]
    fn adapter_rejects_digest_mismatch_and_oversized_reads_before_engine_traversal() {
        let root = tempfile::tempdir().unwrap();
        let store = objects(root.path());
        let id = ObjectId::for_bytes(b"expected");
        let file = store.path(&id);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"wrong").unwrap();
        assert!(matches!(
            store.read(&id),
            Err(ObjectStoreError::ContentMismatch { .. })
        ));
        fs::write(&file, vec![0; MAX_OBJECT + 1]).unwrap();
        assert!(store
            .read(&id)
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
        assert!(store.write(&vec![0; MAX_OBJECT + 1]).is_err());
        store.bytes.set(MAX_BYTES);
        assert!(store.write(b"x").is_err());
    }
    #[test]
    fn malformed_paths_and_terminal_blob_metadata_fail_before_merge() {
        let root = tempfile::tempdir().unwrap();
        let store = objects(root.path());
        let mut r = request(&store.root, &store);
        let blob = write_blob(&store, b"data").unwrap();
        for (name, size) in [("../bad", 4), ("file", 999)] {
            let tree = write_tree(
                &store,
                vec![TreeEntry {
                    name: name.into(),
                    path: name.into(),
                    entry_type: EntryType::File,
                    object: ObjectRef::new(ObjectKind::Blob, blob.id),
                    mode: EntryMode::Normal,
                    size: Some(size),
                    content_hash: None,
                }],
            )
            .unwrap();
            r.inputs.source_snapshot =
                write_snapshot(&store, tree.id, SnapshotOptions::new("repo_test"))
                    .unwrap()
                    .id
                    .to_string();
            assert!(prepare(&r, &store).is_err());
        }
    }
    #[test]
    fn text_complexity_and_deep_trees_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        let store = objects(root.path());
        let mut r = request(&store.root, &store);
        let blob = write_blob(&store, &b"line\n".repeat(2001)).unwrap();
        let tree = write_tree(
            &store,
            vec![TreeEntry {
                name: "file".into(),
                path: "file".into(),
                entry_type: EntryType::File,
                object: ObjectRef::new(ObjectKind::Blob, blob.id),
                mode: EntryMode::Normal,
                size: None,
                content_hash: None,
            }],
        )
        .unwrap();
        r.inputs.source_snapshot =
            write_snapshot(&store, tree.id, SnapshotOptions::new("repo_test"))
                .unwrap()
                .id
                .to_string();
        assert!(prepare(&r, &store)
            .unwrap_err()
            .to_string()
            .contains("line limit"));
        let mut tree = write_tree(&store, Vec::new()).unwrap();
        for depth in (0..66).rev() {
            tree = write_tree(
                &store,
                vec![TreeEntry {
                    name: "x".into(),
                    path: std::iter::repeat_n("x", depth + 1)
                        .collect::<Vec<_>>()
                        .join("/")
                        .into(),
                    entry_type: EntryType::Directory,
                    object: ObjectRef::new(ObjectKind::Tree, tree.id),
                    mode: EntryMode::Directory,
                    size: None,
                    content_hash: None,
                }],
            )
            .unwrap();
        }
        r.inputs.source_snapshot =
            write_snapshot(&store, tree.id, SnapshotOptions::new("repo_test"))
                .unwrap()
                .id
                .to_string();
        assert!(prepare(&r, &store)
            .unwrap_err()
            .to_string()
            .contains("depth limit"));
    }
}
