# Temporary upstream dependency patch

## wgpu-hal 30.0.1: DX12 pipeline stream alignment on 32-bit Windows

- Official source archive: https://static.crates.io/crates/wgpu-hal/wgpu-hal-30.0.1.crate
- Archive SHA-256: `b6b7fb58561a792bc237628ba0792e332de418fefe145f13b5ed8201e6d52f58`
- This digest was checked against the original crates.io checksum in Cargo.lock before extraction.
- Published source commit: `40f4a34ebaf56f9a046231f54125ad046239d3f3`, path `wgpu-hal` (preserved `.cargo_vcs_info.json`).
- All 86 files from the official crate archive are retained, including `LICENSE.MIT`, `LICENSE.APACHE`, the published manifests, lockfile, and source metadata. Cargo registry cache markers and build output are not vendored.
- Only `src/dx12/pipeline_desc.rs` differs from the archive. The exact source-and-test diff is [patches/wgpu-hal-30.0.1-dx12-pointer-alignment.patch](patches/wgpu-hal-30.0.1-dx12-pointer-alignment.patch).

### Reason and evidence

D3D12 requires each pipeline-stream subobject to begin at natural word/pointer alignment, not an unconditional 8-byte boundary: [Microsoft ABI specification](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_pipeline_state_stream_desc#remarks). Version 30.0.1 hard-codes 8 in `RenderPipelineStateStream::add_object`.

With the actual Windows SDK types, root signature plus blend state places the next sample-mask subobject at byte 340 on x86; the original serializer inserts four unwanted bytes and starts at 344. Those zero bytes can be read as an additional root-signature tag. The native 64-bit offset is 352, which the original code already handles. Shader subobject boundaries also differ (20 on x86, 40 on 64-bit).

Both Nebulabook upgrade PRs compiled and passed Rust tests on x86, but the real x86 DX12/WARP startup failed in `Device::create_render_pipeline` / `egui_pipeline` with `0x80070057`. Linux x64/ARM64 and Windows x64/ARM64 startup passed. Original failing native jobs: [PR 5](https://github.com/Tran314/Nebulabook/actions/runs/37827732085/job/113484801925), [PR 6](https://github.com/Tran314/Nebulabook/actions/runs/37827720286/job/113484761982).

### Bounded change and verification

The runtime change uses `align_of::<*const c_void>()` for the subobject start. Natural payload alignment, backend selection, FXC, adapter discovery, shader compilation, and all other vendor code remain unchanged. Two regression tests call the production serializer and compare emitted tag positions with pointer-aligned native SDK layouts. The existing tests remain enabled.

The root manifest uses a visible `[patch.crates-io]` path override. Keep the package name/version and original license/provenance intact; no advisory ignore is added. Security/dependency review must account for this local source override and the exact patch, rather than treating it as an untouched registry copy. The vendored crate's original Cargo.lock is provenance; the application builds with the root Cargo.lock.

At preparation time (2026-10-08), official release 30.0.1 and upstream trunk still contain the hard-coded alignment. This is a temporary local dependency patch, not an upstream-accepted fix. The changed PR's real Windows x86 startup is required to confirm that it repairs the observed failure. No CI check is skipped or relaxed.

### Removal condition

When an official compatible wgpu-hal release includes correct pointer-sized stream alignment, remove the root path override and this vendored copy, update the lockfile to the official registry version, and rerun the ABI regression and all five native CI targets. Do not remove the patch solely because compilation succeeds: genuine x86 renderer startup is the acceptance test. Do not broaden this patch to unrelated vendor changes.
