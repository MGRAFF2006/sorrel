//! Drift guard for the vendored policy conformance manifest.
//!
//! `tests/conformance/policy-conformance.json` is a vendored copy of the
//! canonical `sorrel-protocol/conformance/policy-conformance.json`. The protocol
//! also publishes a sidecar `policy-conformance.meta.json` recording the manifest
//! version and a SHA-256 over the canonical bytes. We vendor the sidecar too.
//!
//! This test recomputes the SHA-256 of the vendored manifest and asserts it
//! matches the vendored sidecar (plus the version fields). If someone edits the
//! vendored manifest by hand without re-exporting from the protocol, the hash no
//! longer matches the sidecar and this test fails. Refresh both files together
//! with `sorrel-protocol`'s `npm run export:conformance -- <this-dir>` (or the
//! root `scripts/sync-conformance.sh`).
//!
//! The check is intentionally self-contained and offline: it needs no network
//! and no cross-repo path, only the two vendored files.

use serde_json::Value;

const MANIFEST: &str = include_str!("conformance/policy-conformance.json");
const META: &str = include_str!("conformance/policy-conformance.meta.json");

/// Minimal, dependency-free SHA-256 (FIPS 180-4) over a byte slice, returned as
/// lowercase hex. Used only to compare the vendored manifest against the sidecar
/// checksum; not used for any security purpose.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: pad to a multiple of 64 bytes.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            let j = i * 4;
            *word = u32::from_be_bytes([chunk[j], chunk[j + 1], chunk[j + 2], chunk[j + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ ((!a[4]) & a[6]);
            let t1 = a[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a[7] = a[6];
            a[6] = a[5];
            a[5] = a[4];
            a[4] = a[3].wrapping_add(t1);
            a[3] = a[2];
            a[2] = a[1];
            a[1] = a[0];
            a[0] = t1.wrapping_add(t2);
        }

        for (hi, ai) in h.iter_mut().zip(a.iter()) {
            *hi = hi.wrapping_add(*ai);
        }
    }

    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

#[test]
fn sha256_implementation_matches_known_vectors() {
    // Guard the embedded SHA-256 against regressions.
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn vendored_manifest_matches_sidecar_checksum() {
    let meta: Value = serde_json::from_str(META).expect("sidecar is valid JSON");
    let manifest: Value = serde_json::from_str(MANIFEST).expect("manifest is valid JSON");

    assert_eq!(meta["kind"], "PolicyConformanceMeta");

    let recorded = meta["sha256"].as_str().expect("sidecar has sha256");
    let actual = sha256_hex(MANIFEST.as_bytes());
    assert_eq!(
        actual, recorded,
        "vendored manifest SHA-256 does not match sidecar; re-export from sorrel-protocol \
         (npm run export:conformance -- <this dir>) instead of hand-editing the manifest"
    );

    assert_eq!(
        meta["manifestVersion"].as_str(),
        manifest["id"].as_str(),
        "sidecar manifestVersion must equal manifest id"
    );
    assert_eq!(
        meta["schemaVersion"].as_str(),
        manifest["schemaVersion"].as_str(),
        "sidecar schemaVersion must equal manifest schemaVersion"
    );
}
