# AethelONE

**AethelONE is a fork of [Polyfrost's OneClient / OneLauncher](https://github.com/Polyfrost/OneLauncher)** — an open source
Minecraft client and launcher — with the official Microsoft-account requirement removed and its own self-hosted
release channel.

## What this fork changes

- **Offline (cracked) accounts everywhere.** Adding, defaulting and launching offline accounts works from the
  onboarding flow, settings, and the launch path — no Microsoft sign-in is required anywhere.
- **Self-hosted auto-updates.** Releases are built and published from
  [aethelreborn/AethelONE](https://github.com/aethelreborn/AethelONE); the app updates itself from them.
  Linux deb/rpm packages and the AppImage verify a minisign signature (public key pinned in
  `packages/oneclient_app/src/constants.rs`); Windows and macOS builds update through the release manifest.
- **Automatic upstream sync.** Polyfrost's upstream is merged every 6 hours by CI. Only `Cargo.lock` conflicts are
  resolved automatically; anything else stops with a GitHub issue instead of being guessed at.

## Installing

Download the latest release from [AethelONE releases](https://github.com/aethelreborn/AethelONE/releases/latest):

| Windows (x86_64) | macOS (Intel & Apple Silicon) | Linux (x86_64) |
|------------------|-------------------------------|----------------|
| Installer 🔄      | DMG 🔄                        | AppImage 🔄     |
|                  | App Bundle 🔄                 | DEB 🔄          |
|                  |                               | RPM 🔄          |

> 🔄 = self-updating: the app checks the AethelONE release channel for new versions.

## Contributing

PRs against this fork are welcome — please read the [contributing guidelines](CONTRIBUTING.md).
Contributions that belong upstream should go to [Polyfrost/OneLauncher](https://github.com/Polyfrost/OneLauncher).

### Requirements

The project targets **Rust 1.97** or later. You can install Rust via [rustup](https://rustup.rs/).

### Building & Running

```sh
# Run the app
cargo run -p oneclient_app

# Build a release binary
cargo build -p oneclient_app --release
```

Debug builds never self-update.

### Packaging / Releasing

Installers are produced with [**cargo-packager**](https://github.com/crabnebula-dev/cargo-packager)
(the standalone bundler spun out of the Tauri bundler). Config lives in
[`packages/oneclient_app/Cargo.toml`](./packages/oneclient_app/Cargo.toml) under
`[package.metadata.packager]`.

```sh
cargo install cargo-packager --locked

# Build the binary, then bundle it for the current OS:
cargo build --release -p oneclient_app
cargo packager --release -p oneclient_app --formats <targets>
#   Windows: nsis      macOS: app,dmg      Linux: deb,rpm,appimage
```

Releases are cut by the **AethelONE Release Build** workflow: add a change file, push, and CI bumps the version,
builds every platform leg, signs the Linux packages, publishes the draft release, and refreshes
`latest.json` / `changelog.json` for the in-app updater.

### Versioning

The workspace shares a single version, defined in the root [`Cargo.toml`](./Cargo.toml) under `[workspace.package]`.

Versions and release notes come from [Knope](https://knope.tech) change files in [`.changeset/`](./.changeset).
Add one per user-facing change with `knope document-change` (installed via `cargo install knope`). The
`AethelONE Release Build` workflow consumes them, bumps the version, writes [`CHANGELOG.md`](./CHANGELOG.md),
and uses the new entry as the GitHub release body, which is what the launcher's changelog page shows.

## Code signing

Linux packages ship with a minisign signature generated in CI from the updater key
(`CARGO_PACKAGER_SIGN_PRIVATE_KEY` secret); the public key is compiled into the app. Windows/macOS installer
signing is optional and stays disabled unless the repository is configured with Apple and SignPath secrets.

## Credits & license

- Fork of [Polyfrost/OneLauncher](https://github.com/Polyfrost/OneLauncher), licensed under
  [GPL-3.0](./LICENSE). Credit and copyright for the original work belong to Polyfrost and its contributors.
- Not affiliated with Polyfrost, Mojang, or Microsoft.
- You are expected to own a legitimate copy of Minecraft.
