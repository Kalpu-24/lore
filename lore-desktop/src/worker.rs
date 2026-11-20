// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! Runs library calls off the UI thread.
//!
//! `lore::call_delegation::run_command` blocks on the library's runtime, so each call runs on a
//! thread of its own. Its events arrive on the library's worker threads and are passed on, as
//! messages, to a sink the caller supplies.

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use lore::auth::LoreAuthLocalUserInfoArgs;
use lore::auth::LoreAuthLoginInteractiveArgs;
use lore::auth::LoreAuthLogoutArgs;
use lore::branch::LoreBranchPushArgs;
use lore::call_delegation::run_command;
use lore::file::LoreFileResetArgs;
use lore::file::LoreFileStageArgs;
use lore::file::LoreFileUnstageArgs;
use lore::interface::LoreArray;
use lore::interface::LoreEvent;
use lore::interface::LoreGlobalArgs;
use lore::interface::LoreRepositoryStatusArgs;
use lore::interface::LoreString;
use lore::remote::command::LoreCommand;
use lore::revision::LoreRevisionCommitArgs;
use lore::revision::LoreRevisionSyncArgs;
use parking_lot::Mutex;

use crate::status::StatusMessage;

/// What a request changes in the instance before its status is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Change nothing: only read the status.
    None,
    /// Stage these paths, relative to the instance's root; the root stages every dirty file.
    Stage(Vec<String>),
    /// Unstage these paths, relative to the instance's root; the root unstages everything.
    Unstage(Vec<String>),
    /// Reset these paths, relative to the instance's root, to the current revision, discarding
    /// their changes.
    Revert(Vec<String>),
    /// Commit the staged files with this message.
    Commit(String),
    /// Bring the instance to its branch's tip, merging when local and remote have diverged.
    Sync,
    /// Push the current branch and its revisions to the remote.
    Push,
    /// Sign in to the instance's remote in the browser.
    LogIn,
    /// Remove every stored identity for the instance's auth service.
    LogOut,
}

/// A call on one repository instance: an optional change, then a status read.
#[derive(Debug, Clone)]
pub struct StatusRequest {
    /// The path of the instance's working tree.
    pub instance_path: String,
    /// Walk the working tree and reconcile it against the current revision, as
    /// `lore status --scan` does, rather than report the tracked state.
    pub scan: bool,
    pub change: Change,
}

/// Runs a request on a thread of its own and passes each message it produces to `sink`, ending
/// with one [`StatusMessage::Complete`].
///
/// A change that fails ends the request with its error before [`StatusMessage::Begin`], so the
/// view keeps the status it had. After a change, the status is read again rather than inferred.
pub fn spawn_status(
    request: StatusRequest,
    sink: impl Fn(StatusMessage) + Send + Sync + 'static,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("lore-desktop-call".into())
        .spawn(move || run_request(request, sink))
}

/// Runs a read-only `command` on the instance at `instance_path`, on a thread of its own, and
/// passes the message `map` makes of each event to `sink`. Its last message is the one `map`
/// makes of the `Complete` event, or the one `fail` makes of the error when the arguments are
/// refused before the call starts.
///
/// Reads may run beside the instance's status call, as the library allows read-only operations
/// to run concurrently.
pub fn spawn_read<M: 'static>(
    instance_path: &str,
    command: LoreCommand,
    map: fn(&LoreEvent) -> Option<M>,
    fail: fn(String) -> M,
    sink: impl Fn(M) + Send + Sync + 'static,
) -> std::io::Result<JoinHandle<()>> {
    let mut globals = LoreGlobalArgs {
        repository_path: instance_path.into(),
        ..Default::default()
    };
    std::thread::Builder::new()
        .name("lore-desktop-read".into())
        .spawn(move || {
            if let Err(err) = globals.validate() {
                sink(fail(err.to_string()));
                return;
            }
            let callback: Box<dyn Fn(&LoreEvent) + Send + Sync> = Box::new(move |event| {
                if let Some(message) = map(event) {
                    sink(message);
                }
            });
            run_command(globals, command, Some(callback));
        })
}

type Sink = Arc<dyn Fn(StatusMessage) + Send + Sync>;

fn run_request(request: StatusRequest, sink: impl Fn(StatusMessage) + Send + Sync + 'static) {
    let sink: Sink = Arc::new(sink);
    let mut globals = LoreGlobalArgs {
        repository_path: request.instance_path.as_str().into(),
        ..Default::default()
    };
    if let Err(err) = globals.validate() {
        sink(StatusMessage::Complete(Err(err.to_string())));
        return;
    }
    if let Some(command) = change_command(&request.instance_path, request.change)
        && let Err(message) = run_change(globals.clone(), command, &sink)
    {
        sink(StatusMessage::Complete(Err(message)));
        return;
    }
    sink(StatusMessage::User(read_user(globals.clone())));
    sink(StatusMessage::Begin);
    let args = LoreRepositoryStatusArgs {
        staged: 1,
        scan: request.scan.into(),
        check_dirty: 0,
        reset: 0,
        sync_point: 0,
        revision_only: 0,
        count: 0,
        paths: LoreArray::default(),
    };
    let callback: Box<dyn Fn(&LoreEvent) + Send + Sync> = Box::new(move |event| {
        if let Some(message) = StatusMessage::from_event(event) {
            sink(message);
        }
    });
    // The call reports its outcome, success or failure, through its `Complete` event.
    run_command(globals, args.into(), Some(callback));
}

/// `path`, relative to the instance's root, as the absolute path the library takes: it resolves
/// a relative path against the process's working directory. The empty path is the root.
pub fn absolute_path(instance_path: &str, path: &str) -> LoreString {
    match path {
        "" => LoreString::from(instance_path),
        path => LoreString::from(
            Path::new(instance_path)
                .join(path)
                .to_string_lossy()
                .as_ref(),
        ),
    }
}

/// The library command for `change`, with its paths made absolute under `instance_path`.
#[lore_macro::test_pub]
fn change_command(instance_path: &str, change: Change) -> Option<LoreCommand> {
    let paths = |paths: Vec<String>| {
        LoreArray::from_vec(
            paths
                .iter()
                .map(|path| absolute_path(instance_path, path))
                .collect(),
        )
    };
    Some(match change {
        Change::None => return None,
        Change::Stage(list) => LoreFileStageArgs {
            paths: paths(list),
            case_change: 0,
            scan: 0,
        }
        .into(),
        Change::Unstage(list) => LoreFileUnstageArgs { paths: paths(list) }.into(),
        Change::Revert(list) => LoreFileResetArgs {
            paths: paths(list),
            // Empty resets to the current revision, as `lore file reset` does without
            // `--revision`.
            revision: LoreString::from(""),
            purge: 0,
        }
        .into(),
        Change::Commit(message) => LoreRevisionCommitArgs {
            message: message.as_str().into(),
            ..Default::default()
        }
        .into(),
        // Empty syncs to the branch tip, as `lore sync` does without a revision.
        Change::Sync => LoreRevisionSyncArgs::default().into(),
        Change::Push => LoreBranchPushArgs {
            branch: LoreString::from(""),
            fast_forward_merge: 0,
        }
        .into(),
        // Empty URLs resolve from the instance's remote.
        Change::LogIn => LoreAuthLoginInteractiveArgs {
            remote_url: LoreString::from(""),
            no_browser: 0,
            reuse_session: 0,
        }
        .into(),
        Change::LogOut => LoreAuthLogoutArgs {
            auth_url: LoreString::from(""),
            resource: LoreString::from(""),
            user_id: LoreString::from(""),
        }
        .into(),
    })
}

/// Runs `command` to completion, passing its progress to `sink`, and returns the error its
/// `Complete` event reports.
fn run_change(globals: LoreGlobalArgs, command: LoreCommand, sink: &Sink) -> Result<(), String> {
    let outcome = Arc::new(Mutex::new(Err(
        "the call ended without reporting".to_owned()
    )));
    let reported = Arc::clone(&outcome);
    let sink = Arc::clone(sink);
    let callback: Box<dyn Fn(&LoreEvent) + Send + Sync> =
        Box::new(move |event| match StatusMessage::from_event(event) {
            Some(StatusMessage::Complete(result)) => *reported.lock() = result,
            Some(message @ StatusMessage::Progress(..)) => sink(message),
            _ => {}
        });
    run_command(globals, command, Some(callback));
    outcome.lock().clone()
}

/// The user the instance's remote knows this client as, from the locally stored token, or
/// `None` when no one is signed in or the call fails.
fn read_user(globals: LoreGlobalArgs) -> Option<String> {
    let user = Arc::new(Mutex::new(None));
    let found = Arc::clone(&user);
    let callback: Box<dyn Fn(&LoreEvent) + Send + Sync> = Box::new(move |event| {
        if let Some(StatusMessage::User(name)) = StatusMessage::from_event(event) {
            *found.lock() = name;
        }
    });
    let args = LoreAuthLocalUserInfoArgs {
        auth_endpoint: LoreString::from(""),
        user_ids: LoreArray::default(),
        with_identity_token: 0,
        with_access_token: 0,
        with_claims: 0,
    };
    // Without a stored token the call fails; that reads as signed out.
    run_command(globals, args.into(), Some(callback));
    user.lock().take()
}
