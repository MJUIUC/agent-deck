# AD-9.4 — macOS Distribution

**Story:** 9.4 — macOS distribution
**Branch:** `feature/phase9-distribution`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9

---

## Summary

Package agent-deck as a proper macOS release. The primary delivery mechanism is a Homebrew formula (hosted in a dedicated tap) so users can install with `brew install` and have the server start automatically on login via `brew services`. A DMG is provided as a secondary option for users who prefer not to use Homebrew. A GitHub Actions workflow produces all release artifacts on tag push.

## Current State

- `server/` — Rust binary that serves the REST API and static web assets from `public/`. Built with `cargo build --release`. The binary and `public/` directory are the only runtime requirements.
- `web/` — Vite/React SPA. `npm run build` outputs to `web/dist/`, which must be copied to `server/public/` before the server is run.
- Data is stored in `~/.agent-deck/`.
- No automated build pipeline exists. No distribution artifacts are produced today.
- No Homebrew tap exists.
- No macOS LaunchAgent configuration exists.

## Implementation Plan

### Task 1 — GitHub Actions release workflow

**File:** `.github/workflows/release.yml` (create)

Triggered on push to any `v*` tag. Steps:

1. `actions/checkout` with `submodules: recursive` so `copilot-api` is available.
2. Install Rust stable via `dtolnay/rust-toolchain`. Add both Apple Silicon and Intel cross-compilation targets: `aarch64-apple-darwin` and `x86_64-apple-darwin`.
3. Install Node 20 via `actions/setup-node`.
4. Build web assets: `cd web && npm ci && npm run build`.
5. Copy `web/dist/` to `server/public/`.
6. Build server for each target:
   - `cargo build --release --target aarch64-apple-darwin`
   - `cargo build --release --target x86_64-apple-darwin`
7. Stage each release artifact: create a temporary directory containing `agent-deck` (the binary) and `public/` (the web assets directory).
8. Create tarballs:
   - `agent-deck-macos-aarch64.tar.gz`
   - `agent-deck-macos-x86_64.tar.gz`
9. Compute SHA256 for each tarball using `shasum -a 256`.
10. Create a GitHub Release using `softprops/action-gh-release`, attaching both tarballs. Include the SHA256 checksums in the release body so they can be pasted directly into the Homebrew formula.

The workflow runs on `macos-latest` so `hdiutil` and code-signing tooling are available if needed in the future.

### Task 2 — Homebrew formula (reference copy)

**File:** `docs/homebrew-formula.rb` (create)

This file is a reference copy. The authoritative formula lives in a separate `homebrew-agent-deck` tap repository (e.g. `github.com/<owner>/homebrew-agent-deck`). The reference copy here is kept in sync manually after each release and serves as the source of truth for the formula structure.

The formula:

- Uses `on_arm` / `on_intel` blocks to reference the correct tarball URL and SHA256 for each architecture.
- `bin.install "agent-deck"` — installs the binary into the Homebrew prefix `bin/`.
- `prefix.install "public"` — installs the `public/` directory adjacent to the binary so the server can locate its web assets at startup.
- `service` stanza:
  - `run [bin/"agent-deck"]`
  - `keep_alive true`
  - `log_path var/"log/agent-deck.log"`
  - `error_log_path var/"log/agent-deck.log"`
- `test` block: asserts that `bin/"agent-deck"` exists and `system bin/"agent-deck", "--version"` exits cleanly (requires adding a `--version` flag to the server binary).
- `def caveats` method returning instructions to the user:
  - Run `brew services start agent-deck` to start the server on login.
  - Open `http://localhost:7474` in a browser to complete setup.
  - Run `brew services stop agent-deck` to stop the server.

File-level comments document:
- How to create the `homebrew-agent-deck` tap repository and add the formula.
- The command users run to add the tap: `brew tap <owner>/agent-deck`.
- The release update workflow: update the `url` and `sha256` fields for each new GitHub Release tag, then open a PR in the tap repo.

### Task 3 — DMG packaging script

**File:** `scripts/build-dmg.sh` (create)

A self-contained shell script for producing a `.dmg` on any macOS machine with Xcode CLI tools installed. No third-party tools required — uses only `hdiutil` (built into macOS).

Steps performed by the script:

1. Verify that `cargo`, `node`, and `npm` are available; exit with a clear message if not.
2. Build web assets: `cd web && npm ci && npm run build`.
3. Copy `web/dist/` to `server/public/`.
4. Build the server binary: `cd server && cargo build --release`.
5. Create a staging directory `build/dmg-staging/agent-deck/` containing:
   - `agent-deck` (the release binary)
   - `public/` (the web assets directory)
   - `README-Install.txt`
6. Write `README-Install.txt` with the following instructions:
   - Copy `agent-deck` to `/usr/local/bin/` (or any directory in `$PATH`).
   - Copy `public/` to the same directory as the binary, or set the `PUBLIC_DIR` environment variable to point to it.
   - Run `agent-deck` directly, or install the LaunchAgent plist from `com.agent-deck.server.plist` (see Task 4) to start on login.
   - Open `http://localhost:7474` to complete first-run setup.
7. Use `hdiutil create` to produce `build/agent-deck.dmg` from the staging directory.

The script accepts an optional `--arch` argument (`aarch64` or `x86_64`) to cross-compile for a specific target when running on a machine with the target toolchain installed. When `--arch` is omitted it builds for the host architecture.

### Task 4 — LaunchAgent plist template

**File:** `scripts/com.agent-deck.server.plist` (create)

A macOS LaunchAgent plist for users who installed via DMG (Homebrew users get this automatically via `brew services`). Placing this file in `~/Library/LaunchAgents/` and running `launchctl load ~/Library/LaunchAgents/com.agent-deck.server.plist` will start the server on login.

Key plist keys:

- `Label`: `com.agent-deck.server`
- `ProgramArguments`: `["/usr/local/bin/agent-deck"]` — user must update this path if the binary is installed elsewhere.
- `RunAtLoad`: `true`
- `KeepAlive`: `true`
- `StandardOutPath`: `~/Library/Logs/agent-deck.log`
- `StandardErrorPath`: `~/Library/Logs/agent-deck.log`

An XML comment block at the top of the file explains how to install, load, unload, and remove the LaunchAgent, and notes that the `ProgramArguments` path must match wherever the binary was placed.

### Schema changes

None.

### Parallelisation note

Tasks 1, 3, and 4 are independent and can be implemented in parallel. Task 2 (Homebrew formula) depends on Task 1 having defined the tarball URL and naming convention, but the formula structure can be drafted first with placeholder URLs and updated once the first release tag is pushed.

---

## Acceptance Criteria

- [ ] `release.yml` workflow triggers on `v*` tag push and completes successfully on `macos-latest`.
- [ ] The workflow produces two tarballs — `agent-deck-macos-aarch64.tar.gz` and `agent-deck-macos-x86_64.tar.gz` — each containing the `agent-deck` binary and the `public/` directory.
- [ ] SHA256 checksums for both tarballs are included in the GitHub Release body.
- [ ] `brew tap <owner>/agent-deck && brew install <owner>/agent-deck && brew services start agent-deck` results in the server running at `http://localhost:7474`.
- [ ] `scripts/build-dmg.sh` runs to completion on a macOS machine with Xcode CLI tools and produces a `.dmg` in `build/`.
- [ ] The DMG's `README-Install.txt` is accurate and sufficient for a user with no prior agent-deck knowledge.
- [ ] `scripts/com.agent-deck.server.plist` correctly starts the server on login when loaded via `launchctl`.

---

## Human Review Instructions

To be filled in after implementation.

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**