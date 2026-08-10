//! Dependency-free micro-benchmarks for the Sorrel engine hot paths.
//!
//! Run with `cargo bench`. This uses a small hand-rolled timing harness
//! (`harness = false`) instead of pulling in a benchmarking crate, keeping the
//! engine's dependency tree minimal. It also enforces coarse perf budgets so a
//! large regression fails the bench (a lightweight CI guard).
//!
//! Budgets are deliberately loose for portability across machines; tune as the
//! engine matures. They exist to catch order-of-magnitude regressions, not to
//! assert precise timings.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use sorrel_core::{
    create_change, materialize_snapshot_excluding, read_snapshot, snapshot_diff, write_snapshot,
    write_tree, ChangeOptions, FileObjectStore, ObjectId, Principal, SnapshotOptions,
};

/// Number of files in the synthetic working tree.
const TREE_FILES: usize = 2_000;
/// Number of changes for the log-walk benchmark.
const LOG_CHANGES: usize = 500;

fn main() {
    println!("sorrel-core engine benchmarks (dependency-free harness)\n");

    bench_snapshot();
    bench_diff();
    bench_log_walk();

    println!("\nAll benchmarks within perf budget.");
}

/// Times a closure over `iters` runs and returns the mean per-iteration time.
fn time<F: FnMut()>(iters: u32, mut body: F) -> Duration {
    // Warm up once so caches/allocations are primed.
    body();
    let start = Instant::now();
    for _ in 0..iters {
        body();
    }
    start.elapsed() / iters
}

fn report(name: &str, mean: Duration, budget: Duration) {
    let within = if mean <= budget { "OK" } else { "OVER BUDGET" };
    println!(
        "{name:<28} mean {:>8.3?}  budget {:>8.3?}  [{within}]",
        mean, budget
    );
    assert!(
        mean <= budget,
        "{name} regressed: mean {mean:?} exceeds budget {budget:?}"
    );
}

/// Writes a synthetic tree of `count` small files under `root`.
fn write_synthetic_tree(root: &Path, count: usize) {
    for i in 0..count {
        let dir = root.join(format!("pkg{:03}", i % 50));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("file{i:05}.txt")),
            format!("content of file {i}\nline two\nline three\n"),
        )
        .unwrap();
    }
}

fn bench_snapshot() {
    let work = tempfile::tempdir().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    write_synthetic_tree(work.path(), TREE_FILES);
    let store = FileObjectStore::new(store_dir.path()).unwrap();

    let mean = time(3, || {
        let options = SnapshotOptions::new("repo_bench");
        materialize_snapshot_excluding(&store, work.path(), [".sorrel"], options).unwrap();
    });
    // ~2k small files materialized + hashed; loose budget.
    report("snapshot 2k files", mean, Duration::from_millis(1_500));
}

fn bench_diff() {
    let store_dir = tempfile::tempdir().unwrap();
    let store = FileObjectStore::new(store_dir.path()).unwrap();

    let base = tempfile::tempdir().unwrap();
    write_synthetic_tree(base.path(), TREE_FILES);
    let base_snap =
        materialize_snapshot_excluding(&store, base.path(), [".sorrel"], SnapshotOptions::new("r"))
            .unwrap();

    // Modify a handful of files in a copy.
    let next = tempfile::tempdir().unwrap();
    write_synthetic_tree(next.path(), TREE_FILES);
    for i in 0..20 {
        let dir = next.path().join(format!("pkg{:03}", i % 50));
        fs::write(
            dir.join(format!("file{i:05}.txt")),
            format!("changed {i}\n"),
        )
        .unwrap();
    }
    let next_snap =
        materialize_snapshot_excluding(&store, next.path(), [".sorrel"], SnapshotOptions::new("r"))
            .unwrap();

    let mean = time(10, || {
        snapshot_diff(&store, &base_snap.id, &next_snap.id).unwrap();
    });
    report("diff 2k files (20 mod)", mean, Duration::from_millis(500));
}

fn bench_log_walk() {
    let store_dir = tempfile::tempdir().unwrap();
    let store = FileObjectStore::new(store_dir.path()).unwrap();

    // Build a linear chain of LOG_CHANGES snapshots.
    let empty_tree = write_tree(&store, Vec::new()).unwrap();
    let mut prev: ObjectId = {
        let mut o = SnapshotOptions::new("repo_log");
        o.message = Some("initial".to_owned());
        write_snapshot(&store, empty_tree.id, o).unwrap().id
    };
    let mut tip = prev;
    for i in 0..LOG_CHANGES {
        let tree = write_tree(&store, Vec::new()).unwrap();
        let mut o = SnapshotOptions::new("repo_log");
        o.message = Some(format!("change {i}"));
        o.parents = vec![sorrel_core::ObjectRef::new(
            sorrel_core::ObjectKind::Snapshot,
            prev,
        )];
        let snap = write_snapshot(&store, tree.id, o).unwrap();
        // Record a change object too, exercising create_change.
        let _ = create_change(
            &store,
            prev,
            snap.id,
            ChangeOptions::new(Principal::system(), format!("change {i}")),
        );
        prev = snap.id;
        tip = snap.id;
    }

    let mean = time(20, || {
        // Walk the first-parent chain from tip to root.
        let mut current = Some(tip);
        let mut count = 0usize;
        while let Some(id) = current {
            let snap = read_snapshot(&store, &id).unwrap();
            count += 1;
            current = snap.parents.first().map(|p| p.id);
        }
        assert_eq!(count, LOG_CHANGES + 1);
    });
    report("log walk 500 changes", mean, Duration::from_millis(300));
}
