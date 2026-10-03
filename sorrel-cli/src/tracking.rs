//! Working-tree selection shared by status, diff, recording and safety checks.

use crate::{repo, CommandOutput};
use clap::Subcommand;
use serde_json::json;
use sorrel_core::{read_snapshot_file_paths, FileObjectStore, ObjectId};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Subcommand)]
pub enum TrackCommand {
    /// Explicitly include an ignored file or directory in future snapshots.
    Add { paths: Vec<PathBuf> },
    /// List explicitly included paths.
    List,
}

pub struct Selection {
    visible: BTreeSet<PathBuf>,
    tracked: BTreeSet<PathBuf>,
    explicit: Vec<PathBuf>,
}

fn explicit_paths_at(root: &Path) -> io::Result<Vec<PathBuf>> {
    match std::fs::read(root.join(".sorrel/tracked.json")) {
        Ok(bytes) => {
            let paths: Vec<String> = serde_json::from_slice(&bytes)?;
            paths
                .into_iter()
                .map(|path| validate_path(Path::new(&path)))
                .collect()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn validate_path(path: &Path) -> io::Result<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(name) => clean.push(name),
            std::path::Component::CurDir => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "tracked paths must stay inside the workspace",
                ))
            }
        }
    }
    let first = clean
        .components()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "tracked path is empty"))?;
    let text = first.as_os_str().to_string_lossy();
    if text.eq_ignore_ascii_case(".sorrel") || text.eq_ignore_ascii_case(".git") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "repository metadata cannot be tracked",
        ));
    }
    Ok(clean)
}

pub fn execute(command: TrackCommand) -> io::Result<CommandOutput> {
    if !repo::is_initialized() {
        return Err(crate::command_error(
            "uninitialized",
            "run `sorrel init` first",
        ));
    }
    let mut paths = explicit_paths_at(Path::new("."))?;
    if let TrackCommand::Add { paths: added } = command {
        if added.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provide at least one path",
            ));
        }
        for path in added {
            let path = validate_path(&path)?;
            std::fs::symlink_metadata(&path)?;
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths.sort();
        let strings: Vec<_> = paths
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        repo::write_json_atomic(&repo::sorrel_dir().join("tracked.json"), &json!(strings))?;
    }
    Ok(CommandOutput {
        json: json!({"command":"track", "paths":paths}),
        human: paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

pub fn selection(store: &FileObjectStore) -> io::Result<Selection> {
    let snapshot = repo::load_head()?
        .filter(|head| !head.snapshot.is_empty())
        .map(|head| head.snapshot.parse::<ObjectId>().map_err(io::Error::other))
        .transpose()?;
    selection_at(store, Path::new("."), snapshot.as_ref())
}

pub fn selection_at(
    store: &FileObjectStore,
    root: &Path,
    snapshot: Option<&ObjectId>,
) -> io::Result<Selection> {
    let tracked = match snapshot {
        Some(id) => read_snapshot_file_paths(store, id).map_err(io::Error::other)?,
        None => BTreeSet::new(),
    };
    let explicit = explicit_paths_at(root)?;
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .require_git(false)
        .git_global(false)
        .add_custom_ignore_filename(".sorrelignore");
    let walk_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        name != ".sorrel"
            && name != ".git"
            && !default_ignored(
                entry
                    .path()
                    .strip_prefix(&walk_root)
                    .unwrap_or(entry.path()),
            )
    });
    let mut visible = BTreeSet::new();
    for entry in builder.build() {
        let entry = entry.map_err(io::Error::other)?;
        if let Some(error) = entry.error() {
            return Err(io::Error::other(error.to_string()));
        }
        if entry.depth() == 0 {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();
        if !default_ignored(&path) && !entry.file_type().is_some_and(|kind| kind.is_dir()) {
            visible.insert(path);
        }
    }
    Ok(Selection {
        visible,
        tracked,
        explicit,
    })
}

fn default_ignored(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), "node_modules" | "target" | "dist")
            || ((name == ".env" || name.starts_with(".env."))
                && ![".example", ".sample", ".template"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix)))
    })
}

impl Selection {
    pub fn includes(&self, path: &Path, directory: bool) -> bool {
        if path
            .components()
            .any(|component| matches!(component.as_os_str().to_str(), Some(".sorrel" | ".git")))
        {
            return false;
        }
        self.tracked.contains(path)
            || (directory && has_descendant(&self.tracked, path))
            || self.explicit.iter().any(|explicit| {
                path.starts_with(explicit) || (directory && explicit.starts_with(path))
            })
            || self.visible.contains(path)
            || (directory && has_descendant(&self.visible, path))
    }
}

fn has_descendant(paths: &BTreeSet<PathBuf>, directory: &Path) -> bool {
    paths
        .range(directory.to_path_buf()..)
        .next()
        .is_some_and(|path| path.starts_with(directory))
}
