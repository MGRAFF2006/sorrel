//! Unix directory-entry barriers for local filesystem publication.
//!
//! File contents must be synchronized separately. Non-Unix platforms retain
//! file flushing and atomic rename without promising directory-entry flushing.

use std::{io, path::Path};

/// Flushes directories from `path` up through the configured `root`, child first.
pub fn flush_directory_chain(path: &Path, root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return flush_chain_with(path, Some(root), &|path| {
        std::fs::File::open(path)?.sync_all()
    });
    #[cfg(not(unix))]
    {
        let _ = (path, root);
        Ok(())
    }
}

/// Flushes the configured root and its ancestors once on writable-store startup.
/// Existing ancestors are included to reconcile earlier failed mkdir barriers.
pub fn flush_root_ancestors(root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return flush_chain_with(root, None, &|path| std::fs::File::open(path)?.sync_all());
    #[cfg(not(unix))]
    {
        let _ = root;
        Ok(())
    }
}

#[cfg(unix)]
fn flush_chain_with(
    path: &Path,
    root: Option<&Path>,
    flush: &dyn Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let cwd = std::env::current_dir()?;
    let path = cwd.join(path);
    let root = root.map(|root| cwd.join(root));
    if root.as_ref().is_some_and(|root| {
        path.strip_prefix(root).map_or(true, |relative| {
            relative
                .components()
                .any(|component| component == std::path::Component::ParentDir)
        })
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory is outside the store root",
        ));
    }
    for directory in path.ancestors() {
        flush(directory)?;
        if root.as_deref() == Some(directory) {
            break;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn publication_barriers_stop_at_owned_root_and_startup_retries_existing_ancestors() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("new/ancestors/store");
        let shard = root.join("objects/ab");
        std::fs::create_dir_all(&shard).unwrap();
        let seen = RefCell::new(Vec::new());
        flush_chain_with(&shard, Some(&root), &|path| {
            seen.borrow_mut().push(path.to_owned());
            std::fs::File::open(path)?.sync_all()
        })
        .unwrap();
        assert_eq!(
            *seen.borrow(),
            vec![shard, root.join("objects"), root.clone()]
        );
        let fail_at = root.parent().unwrap();
        assert!(flush_chain_with(&root, None, &|path| {
            if path == fail_at {
                return Err(io::Error::other("injected ancestor flush failure"));
            }
            std::fs::File::open(path)?.sync_all()
        })
        .is_err());
        seen.borrow_mut().clear();
        // All entries now exist; a later startup must still flush the failed ancestor.
        flush_chain_with(&root, None, &|path| {
            seen.borrow_mut().push(path.to_owned());
            std::fs::File::open(path)?.sync_all()
        })
        .unwrap();
        assert!(seen.borrow().iter().any(|path| path == fail_at));
        assert_eq!(seen.borrow().last().unwrap(), Path::new("/"));
    }

    #[test]
    fn an_outside_directory_is_rejected_before_any_barrier() {
        let outer = tempfile::tempdir().unwrap();
        let called = std::cell::Cell::new(false);
        let root = outer.path().join("store");
        for path in [outer.path().to_owned(), root.join("../outside")] {
            assert_eq!(
                flush_chain_with(&path, Some(&root), &|_| {
                    called.set(true);
                    Ok(())
                })
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert!(!called.get());
    }
}
