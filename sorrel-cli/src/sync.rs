//! HTTP sync transport client for sorrel-hub (see sorrel-protocol sync-transport spec).
//!
//! Development requests send `x-sorrel-acting-principal` as a JSON string. When
//! `SORREL_HUB_TOKEN` is set, every request also carries the bearer token for a
//! Hub configured with OIDC or WorkOS authentication.

use std::io;

use serde_json::{json, Value};
use sorrel_core::{
    collect_closure, parse_object_id_hex, FileObjectStore, ObjectId, ObjectStore, ObjectStoreError,
};

use crate::repo::{self, Head, Remote};

/// Default acting principal for local development calls.
pub const DEFAULT_ACTING_PRINCIPAL: &str = r#"{"type":"user","id":"local"}"#;

/// Environment variable used for a Hub OIDC/WorkOS bearer access token.
pub const HUB_TOKEN_ENV: &str = "SORREL_HUB_TOKEN";

/// Bootstrap grant ids recognized by sorrel-hub's local trusted-grant map
/// (`SORREL_HUB_BOOTSTRAP_GRANTS=1`, opt-in). Matching Hub constants live in
/// `sorrel-hub/src/bootstrap-grants.js`.
pub const BOOTSTRAP_OBJECT_WRITE_GRANT_ID: &str = "grant_local_object_write";
pub const BOOTSTRAP_REF_WRITE_GRANT_ID: &str = "grant_local_ref_write";

/// Result summary from a push operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushResult {
    /// Remote name that was updated.
    pub remote: String,
    /// Ref name advanced on the remote.
    pub ref_name: String,
    /// Local snapshot id that was pushed.
    pub snapshot: String,
    /// Number of objects uploaded.
    pub uploaded: usize,
}

/// Result summary from a pull operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullResult {
    /// Remote name fetched from.
    pub remote: String,
    /// Ref name read on the remote.
    pub ref_name: String,
    /// Snapshot id now at local HEAD.
    pub snapshot: String,
    /// Number of objects downloaded.
    pub downloaded: usize,
}

/// Low-level sync HTTP client bound to one remote repository.
pub struct SyncClient {
    base_url: String,
    repo_id: String,
    agent: ureq::Agent,
    principal_header: String,
    authorization_header: Option<String>,
}

impl SyncClient {
    /// Builds a client for `remote` using a fresh ureq agent.
    #[must_use]
    pub fn new(remote: &Remote) -> Self {
        let token = hub_bearer_token();
        Self::with_auth(remote, DEFAULT_ACTING_PRINCIPAL, token.as_deref())
    }

    /// Builds a client with a custom acting-principal JSON header value.
    #[must_use]
    pub fn with_principal(remote: &Remote, principal_header: &str) -> Self {
        Self::with_auth(remote, principal_header, None)
    }

    /// Builds a client with explicit development principal and bearer token.
    #[must_use]
    pub fn with_auth(remote: &Remote, principal_header: &str, bearer_token: Option<&str>) -> Self {
        let base_url = remote.url.trim_end_matches('/').to_owned();
        Self {
            base_url,
            repo_id: remote.repo_id.clone(),
            agent: ureq::Agent::new(),
            principal_header: principal_header.to_owned(),
            authorization_header: hub_authorization_value(bearer_token),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.base_url, self.repo_id, path)
    }

    fn get_json(&self, path: &str) -> io::Result<Value> {
        let request = apply_hub_authorization(
            self.agent.get(&self.url(path)),
            self.authorization_header.as_deref(),
        );
        let response = request.call().map_err(http_error)?;
        response
            .into_json()
            .map_err(|error| io::Error::other(error.to_string()))
    }

    fn post_json(&self, path: &str, body: &Value) -> io::Result<Value> {
        let request = self
            .agent
            .post(&self.url(path))
            .set("Content-Type", "application/json")
            .set("x-sorrel-acting-principal", &self.principal_header);
        let response = apply_hub_authorization(request, self.authorization_header.as_deref())
            .send_json(body)
            .map_err(http_error)?;
        response
            .into_json()
            .map_err(|error| io::Error::other(error.to_string()))
    }

    /// `GET .../refs` — list remote ref heads.
    pub fn list_refs(&self) -> io::Result<Value> {
        self.get_json("refs")
    }

    /// `POST .../objects/missing` — objects the remote needs to reach `want`
    /// (a snapshot id) given the object ids in `have`.
    pub fn post_missing(&self, want: &ObjectId, have: &[ObjectId]) -> io::Result<Value> {
        let body = json!({
            "want": [want.to_hex()],
            "have": have.iter().map(|id| id.to_hex()).collect::<Vec<_>>(),
        });
        self.post_json("objects/missing", &body)
    }

    /// `POST .../objects` — upload many objects in one request.
    pub fn upload_objects(&self, items: &[(&ObjectId, &[u8])]) -> io::Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let objects: Vec<Value> = items
            .iter()
            .map(|(id, bytes)| {
                json!({
                    "id": id.to_hex(),
                    "bytes": base64_encode(bytes),
                })
            })
            .collect();
        self.post_json(
            "objects",
            &json!({
                "objects": objects,
                "grantRefs": [{
                    "id": BOOTSTRAP_OBJECT_WRITE_GRANT_ID,
                    "source": "core",
                }],
            }),
        )?;
        Ok(())
    }

    /// `GET .../objects/{id}` — download one object as `{ id, bytes }` (base64),
    /// per the sync-transport spec. Verifies the response id matches the request.
    pub fn download_object(&self, id: &ObjectId) -> io::Result<Vec<u8>> {
        let value = self.get_json(&format!("objects/{}", id.to_hex()))?;
        let response_id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing object `id`"))?;
        if response_id != id.to_hex() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("object response id mismatch: expected {id}, got {response_id}"),
            ));
        }
        let encoded = value
            .get("bytes")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing object `bytes`"))?;
        base64_decode(encoded)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid base64 in `bytes`"))
    }

    /// `POST .../refs/{name}` — advance a remote ref to `snapshot` (`force` only false for now).
    ///
    /// Slashes in `ref_name` (e.g. `lane/main`) are URL-encoded per the spec.
    pub fn advance_ref(
        &self,
        ref_name: &str,
        snapshot: &ObjectId,
        force: bool,
    ) -> io::Result<Value> {
        let body = json!({
            "snapshot": snapshot.to_hex(),
            "force": force,
            "grantRefs": [{
                "id": BOOTSTRAP_REF_WRITE_GRANT_ID,
                "source": "core",
            }],
        });
        let encoded_name = ref_name.replace('/', "%2F");
        self.post_json(&format!("refs/{encoded_name}"), &body)
    }
}

pub(crate) fn hub_bearer_token() -> Option<String> {
    std::env::var(HUB_TOKEN_ENV)
        .ok()
        .and_then(|token| normalize_hub_token(&token).map(str::to_owned))
}

pub(crate) fn apply_hub_authorization(
    request: ureq::Request,
    authorization_header: Option<&str>,
) -> ureq::Request {
    match authorization_header {
        Some(value) => request.set("Authorization", value),
        None => request,
    }
}

fn hub_authorization_value(token: Option<&str>) -> Option<String> {
    token
        .and_then(normalize_hub_token)
        .map(|token| format!("Bearer {token}"))
}

fn normalize_hub_token(token: &str) -> Option<&str> {
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Pushes `local_snapshot_id` to `remote` ref `ref_name`.
pub fn push(
    store: &FileObjectStore,
    remote: &Remote,
    remote_name: &str,
    ref_name: &str,
    local_snapshot_id: &ObjectId,
    principal_header: Option<&str>,
) -> io::Result<PushResult> {
    let client = match principal_header {
        Some(header) => SyncClient::with_principal(remote, header),
        None => SyncClient::new(remote),
    };

    let refs = client.list_refs()?;
    let remote_snapshot = ref_snapshot_hex(&refs, ref_name);

    let remote_have_id: Option<ObjectId> = match remote_snapshot.as_deref() {
        Some(hex) if !hex.is_empty() => Some(parse_object_id(hex)?),
        _ => None,
    };

    // The remote cannot walk the closure of a snapshot it does not yet have, so
    // `POST /objects/missing` for a fresh push only reports the snapshot id
    // itself. Compute the full local closure of what we want to push and
    // subtract the objects the remote already has (the closure reachable from
    // the remote ref snapshot, when we hold it locally). Upload the difference.
    let mut want_closure = local_closure(store, local_snapshot_id)?;
    if let Some(remote_id) = remote_have_id {
        if store.has(&remote_id).map_err(store_io)? {
            let remote_closure: std::collections::BTreeSet<ObjectId> =
                local_closure(store, &remote_id)?.into_iter().collect();
            want_closure.retain(|id| !remote_closure.contains(id));
        }
    }

    // Send the remote-known snapshot as `have` so a resumed push (remote already
    // holds part of the closure) does not re-upload objects it reports present.
    let remote_have: Vec<ObjectId> = remote_have_id.into_iter().collect();
    let remote_missing: std::collections::BTreeSet<ObjectId> =
        parse_missing_ids(&client.post_missing(local_snapshot_id, &remote_have)?)?
            .into_iter()
            .collect();
    // Union of (closure we must seed) and (ids the remote explicitly asked for).
    let mut upload_ids: Vec<ObjectId> = want_closure;
    for id in remote_missing {
        if !upload_ids.contains(&id) {
            upload_ids.push(id);
        }
    }

    let mut batch: Vec<(ObjectId, Vec<u8>)> = Vec::with_capacity(upload_ids.len());
    for id in &upload_ids {
        let bytes = store.read(id).map_err(store_io)?;
        batch.push((*id, bytes));
    }
    let upload_items: Vec<(&ObjectId, &[u8])> = batch
        .iter()
        .map(|(id, bytes)| (id, bytes.as_slice()))
        .collect();
    client.upload_objects(&upload_items)?;
    let uploaded = upload_items.len();

    client.advance_ref(ref_name, local_snapshot_id, false)?;

    Ok(PushResult {
        remote: remote_name.to_owned(),
        ref_name: ref_name.to_owned(),
        snapshot: local_snapshot_id.to_hex(),
        uploaded,
    })
}

/// Pulls `ref_name` from `remote` and updates local HEAD to the remote snapshot.
pub fn pull(
    store: &FileObjectStore,
    remote: &Remote,
    remote_name: &str,
    ref_name: &str,
    principal_header: Option<&str>,
) -> io::Result<PullResult> {
    let client = match principal_header {
        Some(header) => SyncClient::with_principal(remote, header),
        None => SyncClient::new(remote),
    };

    let refs = client.list_refs()?;
    let remote_hex = ref_snapshot_hex(&refs, ref_name)
        .ok_or_else(|| io::Error::other(format!("remote ref `{ref_name}` not found")))?;

    let remote_snapshot = parse_object_id(&remote_hex)?;
    let head = repo::load_head()?.ok_or_else(|| io::Error::other("missing HEAD pointer"))?;

    // Let the remote compute which objects we still need: send the remote
    // snapshot as `want` and everything we can already reach locally as `have`.
    // The local store cannot walk the remote closure itself (it may lack those
    // objects), so negotiation happens server-side.
    let local_have = match head_snapshot_id(&head)? {
        Some(local) => local_closure(store, &local)?,
        None => Vec::new(),
    };
    let missing_response = client.post_missing(&remote_snapshot, &local_have)?;
    let missing_ids = parse_missing_ids(&missing_response)?;

    let mut downloaded = 0usize;
    for id in missing_ids {
        if store.has(&id).map_err(store_io)? {
            continue;
        }
        let bytes = client.download_object(&id)?;
        let written = store.write(&bytes).map_err(store_io)?;
        if written != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("downloaded object id mismatch: expected {id}, got {written}"),
            ));
        }
        downloaded += 1;
    }

    let lane = head.lane;
    repo::write_head(&Head {
        lane,
        snapshot: remote_snapshot.to_hex(),
    })?;

    Ok(PullResult {
        remote: remote_name.to_owned(),
        ref_name: ref_name.to_owned(),
        snapshot: remote_snapshot.to_hex(),
        downloaded,
    })
}

/// Collects the object closure for a snapshot id in the local store.
///
/// Returns the ids sorted (from the engine's [`collect_closure`]). Used to build
/// the `have` hint for the remote during pull negotiation.
pub fn local_closure(store: &FileObjectStore, root: &ObjectId) -> io::Result<Vec<ObjectId>> {
    let closure = collect_closure(store, &[*root]).map_err(transport_io)?;
    Ok(closure.into_iter().collect())
}

fn head_snapshot_id(head: &Head) -> io::Result<Option<ObjectId>> {
    if head.snapshot.is_empty() {
        return Ok(None);
    }
    parse_object_id(&head.snapshot).map(Some)
}

fn parse_object_id(hex: &str) -> io::Result<ObjectId> {
    parse_object_id_hex(hex)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))
}

/// Maps an object-store error into an `io::Error` for the sync surface.
fn store_io(error: ObjectStoreError) -> io::Error {
    io::Error::other(error.to_string())
}

/// Maps a transport error into an `io::Error` for the sync surface.
fn transport_io(error: sorrel_core::TransportError) -> io::Error {
    io::Error::other(error.to_string())
}

/// Reads a named ref's snapshot hex from a Hub `GET /refs` payload.
///
/// Protocol shape is `{ "refs": [ { "name", "snapshot" }, ... ] }`. An older
/// object map shape (`{ "refs": { "<name>": { "snapshot" } } }`) is still
/// accepted for compatibility with early mocks.
fn ref_snapshot_hex(value: &Value, ref_name: &str) -> Option<String> {
    let refs = value.get("refs")?;
    if let Some(entries) = refs.as_array() {
        for entry in entries {
            let name = entry.get("name").and_then(Value::as_str)?;
            if name == ref_name {
                return entry
                    .get("snapshot")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
        return None;
    }

    refs.as_object()
        .and_then(|map| map.get(ref_name))
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("snapshot"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_missing_ids(value: &Value) -> io::Result<Vec<ObjectId>> {
    let ids = value
        .get("missing")
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing `missing` array"))?;
    ids.iter()
        .map(|entry| {
            let hex = entry.as_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing id must be string")
            })?;
            parse_object_id(hex)
        })
        .collect()
}

fn http_error(error: ureq::Error) -> io::Error {
    io::Error::other(error.to_string())
}

fn base64_decode(encoded: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let stripped: Vec<u8> = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if stripped.len() % 4 != 0 {
        return None;
    }

    let mut out = Vec::with_capacity(stripped.len() / 4 * 3);
    for chunk in stripped.chunks(4) {
        let padding = chunk.iter().rev().take_while(|&&byte| byte == b'=').count();
        if padding > 2 || chunk[..4 - padding].contains(&b'=') {
            return None;
        }
        let mut triple: u32 = 0;
        for &byte in &chunk[..4 - padding] {
            triple = (triple << 6) | value(byte)?;
        }
        triple <<= 6 * padding as u32;
        out.push((triple >> 16) as u8);
        if padding < 2 {
            out.push((triple >> 8) as u8);
        }
        if padding == 0 {
            out.push(triple as u8);
        }
    }
    Some(out)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn base64_roundtrip_length() {
        let encoded = base64_encode(b"hello");
        assert_eq!(encoded, "aGVsbG8=");
    }

    #[test]
    fn base64_decode_round_trips_various_lengths() {
        for input in [&b""[..], b"a", b"ab", b"abc", b"abcd", b"hello sorrel!"] {
            let encoded = base64_encode(input);
            assert_eq!(base64_decode(&encoded).as_deref(), Some(input));
        }
    }

    #[test]
    fn base64_decode_rejects_invalid_input() {
        assert_eq!(base64_decode("abc"), None);
        assert_eq!(base64_decode("a=bc"), None);
        assert_eq!(base64_decode("!!!!"), None);
    }

    #[test]
    fn bearer_authorization_trims_non_empty_tokens() {
        assert_eq!(
            hub_authorization_value(Some("  access-token  ")).as_deref(),
            Some("Bearer access-token")
        );
        assert_eq!(hub_authorization_value(Some("  ")), None);
        assert_eq!(hub_authorization_value(None), None);
    }

    #[test]
    fn sync_client_forwards_explicit_bearer_token() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            loop {
                let read = stream.read(&mut chunk).expect("read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
            assert!(request.contains("authorization: bearer test-token\r\n"));

            let body = r#"{"refs":[]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write response");
        });

        let remote = Remote {
            url: format!("http://{address}"),
            repo_id: "repo_auth_test".to_owned(),
        };
        let client = SyncClient::with_auth(&remote, DEFAULT_ACTING_PRINCIPAL, Some("test-token"));
        let refs = client.list_refs().expect("authenticated request succeeds");
        assert_eq!(refs["refs"], json!([]));
        server.join().expect("test server joins");
    }
}
