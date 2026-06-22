# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-06-22

### Changed
- **BREAKING:** Replaced the unmaintained `bincode` crate with `postcard` for all
  binary serialization, covering both the network protocol and the on-disk save
  formats. The new wire and on-disk formats are incompatible with prior versions.
  - Network protocol version bumped (`PROTOCOL_SCHEMA_VERSION` 2 → 3,
    `PROTOCOL_VERSION` `voxel-world-3`); clients on an older binary are rejected at
    the netcode handshake.
  - On-disk save formats bumped (VXM 2 → 3, VXS 1 → 2, VXT 1 → 2, world
    `FORMAT_VERSION` 3 → 4). Existing bincode-serialized worlds, models, stencils,
    and templates will not load.
- Upgraded `nalgebra` 0.34 → 0.35.
- Bumped compatible dependencies to their latest releases: `winit` 0.30.13,
  `serde_json` 1.0.150, `tokio` 1.52.3, `log` 0.4.33, `socket2` 0.6.4,
  `bytes` 1.12.0, plus transitive updates.
- Updated GitHub Actions workflows to the latest major versions.

### Security
- Inbound network messages remain bounded by `MAX_INBOUND_MESSAGE_SIZE`, now
  enforced as a raw-length cap before decode (postcard has no built-in decode
  limit; the input slice bounds deserialization).

### Notes
- `shaderc` stays pinned at 0.8.3 — it is required transitively by the
  vulkano 0.35 shader stack, and a 0.10 bump conflicts on the native `shaderc`
  link.

## [0.1.0]

- Initial development release.
