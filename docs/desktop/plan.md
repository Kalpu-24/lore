# Plan: Rust desktop client (`lore-desktop`)

Design and rationale: `docs/proposals/2026-11-19-open-source-rust-desktop-client.md`.
Toolkit: egui/eframe, recorded in `docs/developing/decisions/00042-desktop-client-toolkit-egui.md`.
This fork is for personal use: no upstream review, sign-off or LEP acceptance gates.

## Phase 0 — Proposal
- [x] Draft the LEP.
- [ ] Fill its remaining `{TODO}`s.

## Phase 1 — Skeleton
- [x] `lore-desktop` crate in the workspace; depends on `lore` only (plus egui).
- [x] Toolkit ADR: egui.
- [x] Plumbing: each call on its own thread via `run_command`, events copied into messages on a
      channel, UI repaints on each; errors from the `Complete` event shown in the window.
- [x] Read-only status view: refresh / scan, branch and remote state, virtualised file tables.
- [x] Measure frame time on a large instance. `tests/unit/app.rs` draws the status headless: 100 and
      1,000,000 rows both draw 136 shapes, at 0.14 and 0.22 ms of CPU a frame (release, Windows).
- [x] `cargo deny` passes with egui's dependency tree: edition 2024 like the workspace, and a
      `deny.toml` exception for `epaint_default_fonts` (OFL-1.1, Ubuntu-font-1.0).

## Phase 2 — Daily loop
- [x] Open/close instances (tabs), remembered in per-user config: eframe's `persistence`
      storage, saved on exit; the path given on the command line opens as the active tab.
- [x] Changes view: status (virtualised), stage/unstage, revert, commit with message. Rows select
      by click; Stage/Unstage take the selection or, with none, the whole instance; Revert asks
      first. A change that fails keeps the last status; after one that succeeds the status is read
      again. Checked by hand against a local `loreserver`.
- [x] Sync and push, with progress (files synced, fragments uploaded). No cancel: the library has
      no way to cancel a single call yet.
- [x] Accounts: login/logout via `lore::auth`. The signed-in user is read from the stored token
      before each status read; Log in runs the browser login, Log out removes the stored
      identities for the instance's auth service. Sync, push and the remote state checked by hand
      against a local `loreserver`; login needs a server with auth configured and is unchecked.

## Phase 3 — History and branches
- [x] History view; revision details. Built on `revision history` (entries with their message,
      time and creator) and `revision info` with deltas for the selected revision's files, both
      read beside the status call. Reloads after a commit, sync or push. Checked by hand against a
      local `loreserver`.
- [ ] Diff view: text side-by-side, binary as metadata; size cap.
- [ ] Branches: list, create, switch; merge with conflict list and resolve mine/theirs.

## Phase 4 — Collaboration
- [ ] Locks view: holders, lock/unlock.
- [ ] Live updates through `lore::notification`; refresh-on-focus fallback.

## Phase 5 — Ship
- [ ] Packaging per platform; CI build of the client.
- [ ] Docs: how-to page and tutorial; release note in `docs/release-notes/`.

## Each change
Follow the repo's code standards: tests in `tests/unit/`, `cargo clippy --all-targets -- -D warnings --no-deps`.

Run: `cargo run --release -p lore-desktop -- <instance path>`
