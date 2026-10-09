# Nebulabook `.nebula` format v1

## Security and scope

This is **password-free lightweight obfuscation and integrity checking**, not confidentiality, password protection, or an attacker-resistant signature. The compatibility key is public and identical in every application copy. Someone who has the source or binary can decrypt, edit and re-encode any file with a valid tag. The format catches accidental corruption and ordinary direct-text editing; it does not detect a valid older file being replayed. It is not suitable for protecting secrets from other computer users. Use operating-system full-disk encryption and private account permissions for sensitive notes.

There is no password prompt, OS keychain entry, persistent credential, network service or machine-bound secret. Portable exports can be imported on another machine. Preserved legacy JSON and explicit TXT/Markdown/JSON exports remain plaintext. No embedded HTML or scripts are executed.

## Binary layout

All integer fields are unsigned little-endian. There is exactly one envelope, with no padding or trailing bytes.

| Offset | Bytes | Meaning |
|---|---:|---|
| 0 | 8 | Magic `4e 45 42 55 4c 41 0d 0a`, ASCII `NEBULA\r\n` |
| 8 | 2 | Envelope version, exactly `1` |
| 10 | 2 | Reserved flags, exactly `0` |
| 12 | 24 | XChaCha20-Poly1305 nonce |
| 36 | 8 | Ciphertext length **including** its 16-byte tag |
| 44 | variable | Ciphertext followed by its 16-byte Poly1305 tag |

The entire 44-byte header is associated authenticated data (AAD). Algorithm: XChaCha20-Poly1305 with 20 ChaCha rounds. The 32-byte key is the exact ASCII byte string `Nebulabook public format key v1!` (one exclamation mark; no newline or NUL). Key disclosure is intentional compatibility behavior, not a secret-management mechanism.

Production encoding obtains a new 24-byte nonce directly through `getrandom::fill`, using the OS CSPRNG. There is no timestamp/counter/UUID-derived nonce and no zero/randomness-failure fallback. CSPRNG failure returns a visible error before persistence. Random 192-bit nonces make accidental cross-installation collisions negligible, not mathematically impossible. Fixed nonces exist only in unit-test fixtures. An unchanged storage save leaves the existing ciphertext and backup generation unchanged.

The decrypted bytes are compact UTF-8 JSON:

```json
{"notebook":{"schema_version":1,"notes":[],"folders":[],"tags":[]},"legacy_source_sha256":null}
```

Both JSON envelope fields are required; a null association is explicit. `notebook` uses the existing native schema (including IDs, dates, folders, tags, trash, inert original HTML and legacy metadata). `legacy_source_sha256` is null or a 64-character lowercase hex SHA-256 of the untouched source JSON **bytes**, not normalized JSON. Local automatic migration sets it; portable exports clear it. Imports read notebook content but never adopt another machine's migration association.

## Validation and limits

- Encrypted payload: at most 64 MiB. Total file: at most 64 MiB + 60 bytes. Legacy/text imports: at most 64 MiB. HTML conversion: at most 8 MiB per item.
- Reject wrong magic, unknown version, nonzero flags, inconsistent/truncated/oversized lengths and all trailing bytes before decryption. Do not infer a format from filename content or fall back to JSON after a `.nebula` failure.
- Verify authentication before JSON parsing or exposing any notes. Authentication errors do not identify a purported key or leak partial content.
- Reject duplicate JSON keys, unknown typed fields, unsupported Notebook schema, invalid IDs/dates/references/cycles and inconsistent trash states.
- At most 100,000 total top-level records (notes, folders, tags, retained settings and archives); each JSON array/object and each note's tag references is bounded to 100,000 entries. Retained metadata nesting is limited to 64 levels, in addition to serde JSON's input recursion bound.
- Parse and validate the full candidate, including interactions with existing data and the resulting serialized size, before replacing the caller's notebook. Storage failures leave UI drafts available for export.

These are safeguards for a small offline notebook, not a claim of resistance to arbitrary hostile local processes. Whole-notebook serialization, encryption and validation are linear in content size, use memory proportional to that size and may briefly pause the UI for large notebooks.

## Files, migration and concurrency

The original `ProjectDirs` identity and data directory are unchanged. Primary: `notebook.nebula`. Previous snapshot: `notebook.backup.nebula`. Initial migration recovery snapshot: `notebook.migration.nebula`. Lock: **`notebook.json.lock`**, shared with previous native versions. The storage API accepts only lowercase `.nebula` and legacy `.json` paths, avoiding ambiguous case-sensitive source discovery; explicit file imports remain case-insensitive.

For automatic migration, acquire the original OS-held lock, read/validate the complete legacy JSON, encode and decode-verify a candidate, verify the source bytes still match their recorded hash, publish a recovery snapshot, recheck the source, then publish the new primary. Legacy source JSON and any `.json.bak` are never overwritten, renamed or deleted. Unsupported/custom native fields fail closed; known inert legacy archives preserve arbitrary supported old metadata without execution.

Publication writes a private same-directory temporary file, syncs it, then hard-links it to a new destination without replacement. Replacement of an existing primary/previous snapshot uses same-directory atomic rename; the destination is never removed first. Best-effort directory sync follows. A crash after recovery publication but before primary publication requires explicit recovery, not implicit retry over possible user data. A crash before either publication leaves only disposable temporary data and the intact source.

Hard-link support is required for first-save/migration publication. Default ext4/NTFS-style local application data directories support this; FAT/exFAT or filesystems rejecting links fail visibly and preserve existing data. There is no unsafe copy/delete fallback. This is not a guarantee of power-failure durability on every filesystem.

If a primary exists, it is authoritative and never silently falls back to stale JSON, even when corrupt. If a primary is absent but a native recovery snapshot exists, stop and request recovery. A missing legacy primary with `.json.bak` also stops rather than creating an empty notebook.

Before each changed or unchanged save, compare the primary's exact bytes against the last opened/saved bytes and check the legacy source association. Recheck after backup I/O. A new or changed legacy JSON means potential downgrade edits and stops opening/saving. Users can close all versions, keep copies, move conflicting JSON elsewhere, reopen the native file and explicitly import desired content. Removing/moving preserved JSON is allowed; the source hash stays in subsequent saves, so a changed source reappearing is still detected. If no migration hash exists, any newly appearing legacy JSON is a conflict.

The cooperative lock and external-change checks prevent ordinary competing instances/editors, not an adversarial process racing inside the final filesystem operation. Do not delete lock files while either version is running. Windows files inherit the user-directory ACL; Unix newly created directories use 0700 and new primary/recovery/lock/export files use 0600. Existing directory permissions are not changed.

## Recovery

Close every application version. Keep the damaged/conflicting files. Copy a confirmed-good `notebook.backup.nebula` to `notebook.nebula`; if the failure occurred around migration, `notebook.migration.nebula` is another candidate but contains only the original migration state. Reopen and verify content. Never overwrite your only copy during recovery. Portable `.nebula` exports can instead be imported into a separate working notebook. Independent backups are still necessary.

## Dependencies and reproducibility

- [`chacha20poly1305` 0.11.0](https://docs.rs/crate/chacha20poly1305/0.11.0): RustCrypto, Apache-2.0 OR MIT, pure Rust; allocation feature only. Upstream documents an NCC Group review of this implementation family; this does **not** mean this application/protocol or this exact release has independently passed that audit.
- [`sha2` 0.11.0](https://docs.rs/crate/sha2/0.11.0): RustCrypto SHA-256, Apache-2.0 OR MIT; used for non-secret source/content fingerprints.
- [`getrandom` 0.4.3](https://docs.rs/crate/getrandom/0.4.3): existing dependency now directly used for fallible OS randomness; Apache-2.0 OR MIT.

The lockfile pins exact versions and checksums. This change adds 17 pure-Rust packages totaling 582,176 bytes of compressed registry source archives (not the executable-size delta); all are MIT/Apache-2.0 licensed. File framing adds 60 bytes plus the small JSON wrapper. There is no OpenSSL or separately installed cryptography-library dependency; OS randomness services remain required (including Windows bcryptprimitives.dll via getrandom). The GUI remains eframe 0.36.2, and the audited local wgpu-hal x86 compatibility patch is unchanged.

`tests/fixtures/empty-v1.nebula` is 155 bytes. Its nonce is 00 through 17 inclusive (hex), and its plaintext is exactly the compact empty JSON shown above. The fixture was constructed using Python cryptography's ChaCha20-Poly1305 with the specified HChaCha20-derived subkey, then matched byte-for-byte by RustCrypto. The test-only stdlib reader in `scripts/smoke_fixtures.py` independently authenticates it and includes published ChaCha20, HChaCha20, Poly1305 and XChaCha20-Poly1305 vectors. It is not application code or a security bypass.

```sh
cargo +1.95.0 test --locked --no-default-features
python3 scripts/smoke_fixtures.py
```

Rust tests cover Unicode/metadata round trips, randomized encodings, every-byte modifications/truncation of the fixture, version/flag/tag/length rejection, transactional imports, full legacy migration and byte-preserved sources, default suffixes, filesystem permissions, shared old/new locks, conflicts, interrupted migrations and explicit recovery. Real GUI smoke tests continue decoding actual saved title/body and preserving external conflict bytes; checking only file existence is insufficient.
