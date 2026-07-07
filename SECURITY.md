# Security Policy

## Reporting a vulnerability

Please report security issues privately rather than as a public issue. Contact
the maintainer by opening a private security advisory on GitHub
(<https://github.com/paulrobello/voxel-world/security/advisories/new>) or email
the address on the maintainer's GitHub profile. Include reproduction steps and,
if possible, the `make run-debug` output. You will receive an acknowledgement
within a reasonable timeframe; please avoid public disclosure until a fix is
released.

## Scope

voxel-world is a single-player-first voxel engine with optional LAN
multiplayer. The multiplayer surface is the main security-relevant component.

## Multiplayer authentication posture

Remote multiplayer uses [renet](https://crates.io/crates/renet) netcode
**Secure mode** with a per-session 32-byte private key generated at startup.
Clients authenticate with a `ConnectToken` signed by that key.

- The host's key is **not** negotiated over the wire. Remote clients must obtain
  it **out-of-band** via a 64-hex-character pairing code displayed on the host
  console. Anyone who has the pairing code can join the session; the host must
  share it only with intended players over a trusted channel.
- The host's loopback client receives the key directly in-process and bypasses
  anti-cheat position validation; remote clients are validated.
- Inbound messages are bounded by `MAX_INBOUND_MESSAGE_SIZE`, enforced as a
  raw-length cap before decode, to limit hostile-peer memory exposure.

This protects against passive eavesdropping and unauthorized clients on the LAN,
but it is **not** a hardened server-grade auth system: there is no per-player
identity, no rate limiting beyond renet's defaults, and no authorization model
beyond "anyone holding the pairing code is a full player." Treat the multiplayer
mode as a trusted-friends-on-a-LAN feature, not as internet-facing.

## What is explicitly out of scope

- Single-player worlds are local files; their integrity is only as good as the
  user's filesystem permissions.
- Custom textures, models, stencils, and templates loaded from disk are trusted
  inputs. Do not load asset files from untrusted sources.

See [docs/NETWORKING.md](docs/NETWORKING.md) for the wire-level authentication
details.
