//! The OS-lock owner performs every protected mutation. Losing it cannot leave
//! the Node transport able to write through a stale liveness observation.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    time::Duration,
};
const MAX_RPC: u64 = 32 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    token: String,
    command: Command,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
enum Command {
    Ping,
    Write {
        root: PathBuf,
        target: PathBuf,
        hex: String,
        guard: Option<Box<Guard>>,
        fault: Option<Fault>,
    },
    Core {
        request: Box<super::Request>,
    },
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    BeforeRename,
    AfterRename,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RefGuard {
    path: PathBuf,
    snapshot: String,
    generation: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Guard {
    target: RefGuard,
    source: RefGuard,
    authority_valid_until: Option<String>,
}
struct WriteFailure {
    source: io::Error,
    published: bool,
}
pub(super) fn contained(root: &Path, path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.strip_prefix(root).map_or(true, |p| {
            p.components().any(|c| matches!(c, Component::ParentDir))
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "write outside locked root",
        ));
    }
    // Dataset roots are canonicalized before locking. Reject descendant aliases
    // before any read or directory creation can reach another writer's dataset.
    let mut current = root.to_path_buf();
    for component in std::iter::once(None).chain(
        path.strip_prefix(root)
            .expect("validated prefix")
            .components()
            .map(Some),
    ) {
        if let Some(component) = component {
            current.push(component);
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "symlink within protected dataset",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn check_ref(root: &Path, guard: &RefGuard) -> io::Result<()> {
    contained(root, &guard.path)?;
    let file = File::open(&guard.path)?;
    let mut bytes = Vec::new();
    file.take((super::MAX_WIRE + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > super::MAX_WIRE {
        return Err(io::Error::other("ref byte limit"));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["snapshot"] != guard.snapshot
        || value
            .get("generation")
            .and_then(Value::as_str)
            .unwrap_or("legacy")
            != guard.generation
    {
        return Err(io::Error::other("publication ref compare-and-swap failed"));
    }
    Ok(())
}
fn atomic_write(
    root: &Path,
    target: &Path,
    bytes: &[u8],
    guard: Option<Box<Guard>>,
    fault: Option<Fault>,
) -> Result<(), WriteFailure> {
    let mut published = false;
    let result = (|| -> io::Result<()> {
        contained(root, target)?;
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::other("missing parent"))?;
        fs::create_dir_all(parent)?;
        contained(root, &fs::canonicalize(parent)?)?;
        let temp = parent.join(format!(
            ".tmp-native-{}-{}",
            std::process::id(),
            jiff::Timestamp::now().as_nanosecond()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let written = (|| -> io::Result<()> {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            if let Some(guard) = guard {
                check_ref(root, &guard.target)?;
                check_ref(root, &guard.source)?;
                if let Some(until) = guard.authority_valid_until {
                    let until = until.parse::<jiff::Timestamp>().map_err(io::Error::other)?;
                    if jiff::Timestamp::now() >= until {
                        return Err(io::Error::other("publication authority lifetime changed"));
                    }
                }
            }
            if fault.is_some() && !cfg!(debug_assertions) {
                return Err(io::Error::other("fault injection unavailable"));
            }
            if matches!(fault, Some(Fault::BeforeRename)) {
                return Err(io::Error::other("injected file barrier failure"));
            }
            fs::rename(&temp, target)?;
            published = true;
            if matches!(fault, Some(Fault::AfterRename)) {
                return Err(io::Error::other("injected directory barrier failure"));
            }
            sorrel_core::durability::flush_directory_chain(parent, root)
        })();
        if temp.exists() {
            fs::remove_file(temp)?;
        }
        written
    })();
    result.map_err(|source| WriteFailure { source, published })
}
fn error_code(error: &io::Error) -> &'static str {
    if matches!(error.raw_os_error(), Some(code) if (cfg!(target_os = "linux") && code == 36) || (cfg!(target_os = "macos") && code == 63) || (cfg!(windows) && code == 206))
    {
        return "ENAMETOOLONG";
    }
    match error.kind() {
        io::ErrorKind::AlreadyExists => "EEXIST",
        io::ErrorKind::NotFound => "ENOENT",
        io::ErrorKind::PermissionDenied => "EACCES",
        io::ErrorKind::NotADirectory => "ENOTDIR",
        io::ErrorKind::IsADirectory => "EISDIR",
        io::ErrorKind::StorageFull => "ENOSPC",
        _ => "EIO",
    }
}
fn command(command: Command, roots: &[PathBuf]) -> Value {
    match command {
        Command::Ping => json!({"ok":true}),
        Command::Core { request } => {
            if !roots
                .iter()
                .any(|root| contained(root, &request.root).is_ok())
            {
                return json!({"error":{"message":"Core object root is outside the locked dataset"}});
            }
            match super::execute(*request) {
                Ok(value) => match serde_json::to_vec(&value) {
                    Ok(bytes) if bytes.len() <= super::MAX_WIRE => json!({"value":value}),
                    _ => json!({"error":{"message":"Core integration output limit"}}),
                },
                Err(error) => json!({"error":{"message":error.to_string()}}),
            }
        }
        Command::Write {
            root,
            target,
            hex,
            guard,
            fault,
        } => {
            if !roots.contains(&root)
                || hex.len() % 2 != 0
                || !hex.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return json!({"error":{"code":"EACCES","message":"invalid protected write"}});
            }
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|offset| {
                    u8::from_str_radix(&hex[offset..offset + 2], 16).expect("validated hex")
                })
                .collect();
            match atomic_write(&root, &target, &bytes, guard, fault) {
                Ok(()) => json!({"ok":true}),
                Err(error) => {
                    json!({"error":{"code":error_code(&error.source),"message":"protected filesystem write failed","published":error.published}})
                }
            }
        }
    }
}
pub(super) fn serve(roots: impl Iterator<Item = String>) -> super::Result<()> {
    let mut roots = roots
        .map(|root| {
            fs::create_dir_all(&root)?;
            fs::canonicalize(root)
        })
        .collect::<io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    if roots.is_empty() || roots.len() > 2 {
        return Err("one or two filesystem roots required".into());
    }
    let mut files = Vec::new();
    for root in &roots {
        contained(root, &root.join(".sorrel-hub-writer.lock"))?;
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
    let mut input = BufReader::new(io::stdin());
    let mut token = String::new();
    input.by_ref().take(129).read_line(&mut token)?;
    let token = token.trim().to_owned();
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid private writer channel".into());
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    println!(
        "{}",
        json!({"locked":true,"port":listener.local_addr()?.port()})
    );
    io::stdout().flush()?;
    // EOF belongs to the Node host. Parent death closes it even during a mutation;
    // process exit releases the OS locks and atomic files/receipts provide recovery.
    std::thread::spawn(move || {
        let _ = io::copy(&mut input.take(1), &mut io::sink());
        std::process::exit(0);
    });
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let response = (|| -> super::Result<Value> {
            let mut bytes = Vec::new();
            (&mut stream).take(MAX_RPC + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_RPC {
                return Err("writer request byte limit".into());
            }
            let request: Envelope = serde_json::from_slice(&bytes)?;
            if request.token != token {
                return Err("private writer channel rejected".into());
            }
            Ok(command(request.command, &roots))
        })()
        .unwrap_or_else(|_| json!({"error":{"message":"private writer request failed"}}));
        let bytes = serde_json::to_vec(&response)?;
        if bytes.len() as u64 <= MAX_RPC {
            let _ = stream.write_all(&bytes);
        }
    }
    drop(files);
    Ok(())
}
pub(super) fn client(port: u16) -> super::Result<()> {
    let mut bytes = Vec::new();
    io::stdin().take(MAX_RPC + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RPC {
        return Err("writer request byte limit".into());
    }
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        Duration::from_secs(2),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(12)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(&bytes)?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    stream.take(MAX_RPC + 1).read_to_end(&mut response)?;
    if response.len() as u64 > MAX_RPC {
        return Err("writer response byte limit".into());
    }
    io::stdout().write_all(&response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn protected_writes_reject_descendant_symlinks_before_creating_directories() {
        for name in ["projects", "repo/objects", "repo/refs"] {
            let root = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            let link = root.path().join(name);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(outside.path(), &link).unwrap();
            let failure = atomic_write(root.path(), &link.join("new/record"), b"never", None, None)
                .err()
                .unwrap();
            assert!(!failure.published);
            assert_eq!(failure.source.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn native_write_guards_and_barrier_failures_never_fabricate_publication() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("main");
        let source = root.path().join("source");
        fs::write(&target, br#"{"snapshot":"target","generation":"one"}"#).unwrap();
        fs::write(&source, br#"{"snapshot":"source","generation":"two"}"#).unwrap();
        let guard = || {
            Box::new(Guard {
                target: RefGuard {
                    path: target.clone(),
                    snapshot: "target".into(),
                    generation: "one".into(),
                },
                source: RefGuard {
                    path: source.clone(),
                    snapshot: "source".into(),
                    generation: "two".into(),
                },
                authority_valid_until: Some("2000-01-01T00:00:00Z".into()),
            })
        };
        let failure = atomic_write(root.path(), &target, b"candidate", Some(guard()), None)
            .err()
            .unwrap();
        assert!(!failure.published);
        assert_eq!(
            fs::read(&target).unwrap(),
            br#"{"snapshot":"target","generation":"one"}"#
        );
        let failure = atomic_write(
            root.path(),
            &target,
            b"candidate",
            None,
            Some(Fault::BeforeRename),
        )
        .err()
        .unwrap();
        assert!(!failure.published);
        let failure = atomic_write(
            root.path(),
            &target,
            b"candidate",
            None,
            Some(Fault::AfterRename),
        )
        .err()
        .unwrap();
        assert_eq!(failure.published, cfg!(debug_assertions));
        assert_eq!(
            fs::read(&target).unwrap(),
            if cfg!(debug_assertions) {
                b"candidate".as_slice()
            } else {
                br#"{"snapshot":"target","generation":"one"}"#.as_slice()
            }
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        let outside = root.path().join("../outside");
        assert!(atomic_write(root.path(), &outside, b"never", None, None).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_filename_length_errors_retain_the_public_transport_code() {
        assert_eq!(
            error_code(&io::Error::from_raw_os_error(36)),
            "ENAMETOOLONG"
        );
    }
}
