// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! One repository instance's tab: its status, changes, sync and account.

use std::sync::mpsc;

use eframe::egui;
use egui_extras::Column;
use egui_extras::TableBuilder;
use lore::branch::LoreBranchListArgs;
use lore::file::DEFAULT_CONTEXT_LINES;
use lore::file::LoreFileDiffArgs;
use lore::interface::LoreArray;
use lore::lock::LoreLockFileQueryArgs;
use lore::notification::LoreNotificationSubscribeArgs;
use lore::notification::LoreNotificationUnsubscribeArgs;
use lore::revision::LoreRevisionHistoryArgs;
use lore::revision::LoreRevisionInfoArgs;

use crate::branches::BranchMessage;
use crate::branches::BranchModel;
use crate::diff::DIFF_CAP;
use crate::diff::DiffMessage;
use crate::diff::DiffModel;
use crate::diff::RowKind;
use crate::history::HistoryMessage;
use crate::history::HistoryModel;
use crate::locks::LockMessage;
use crate::locks::LockModel;
use crate::notify::Live;
use crate::notify::LiveModel;
use crate::notify::NotifyMessage;
use crate::status::Progress;
use crate::status::Section;
use crate::status::StatusMessage;
use crate::status::StatusModel;
use crate::status::action_label;
use crate::worker::Change;
use crate::worker::StatusRequest;
use crate::worker::absolute_path;
use crate::worker::spawn_read;
use crate::worker::spawn_status;

const ROW_HEIGHT: f32 = 18.0;

/// One repository instance and its status.
#[lore_macro::test_pub]
pub struct Instance {
    instance_path: String,
    status: StatusModel,
    /// Messages of the running call, sent from the library's threads.
    messages: Option<mpsc::Receiver<StatusMessage>>,
    commit_message: String,
    /// The running call commits: its message is cleared once the commit succeeds.
    committing: bool,
    /// The running call moves the branch or changes which one is current, so the history and
    /// branches are read again once it succeeds.
    moves_branch: bool,
    /// The running call takes or releases locks, so the locks are read again once it succeeds.
    changes_locks: bool,
    /// A change that discards work, waiting for the user to confirm it.
    pending_confirm: Option<Change>,
    view: View,
    history: HistoryModel,
    /// Messages of the running history call.
    history_messages: Option<mpsc::Receiver<HistoryMessage>>,
    /// Messages of the running info call on the selected revision.
    changes_messages: Option<mpsc::Receiver<HistoryMessage>>,
    /// The diff shown beside the view it was opened from.
    diff: Option<(View, DiffModel)>,
    diff_messages: Option<mpsc::Receiver<DiffMessage>>,
    branches: BranchModel,
    branch_messages: Option<mpsc::Receiver<BranchMessage>>,
    /// The name typed for a new branch.
    new_branch: String,
    locks: LockModel,
    lock_messages: Option<mpsc::Receiver<LockMessage>>,
    /// The path typed to lock, relative to the instance's root.
    lock_path: String,
    live: LiveModel,
    live_messages: Option<mpsc::Receiver<NotifyMessage>>,
    /// When the status was last read.
    updated: Option<chrono::DateTime<chrono::Local>>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

/// The view a tab shows below its toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Changes,
    History,
    Branches,
    Locks,
}

/// Takes every message waiting in `slot`'s channel and passes it to `apply`. Once the call's
/// thread has ended and its messages are taken, empties `slot` and returns true.
fn drain<M>(slot: &mut Option<mpsc::Receiver<M>>, mut apply: impl FnMut(M)) -> bool {
    let Some(receiver) = slot else {
        return false;
    };
    loop {
        match receiver.try_recv() {
            Ok(message) => apply(message),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                *slot = None;
                return true;
            }
        }
    }
}

/// A channel whose sender asks `ctx` to repaint after each message, so the window draws it.
fn repainting_channel<M: Send + 'static>(
    ctx: &egui::Context,
) -> (impl Fn(M) + Send + Sync + 'static, mpsc::Receiver<M>) {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    let sink = move |message| {
        // The tab may have closed; nothing is left to show the message then.
        let _ = sender.send(message);
        repaint.request_repaint();
    };
    (sink, receiver)
}

impl Instance {
    pub fn new(instance_path: String) -> Self {
        Self {
            instance_path,
            status: StatusModel::default(),
            messages: None,
            commit_message: String::new(),
            committing: false,
            moves_branch: false,
            changes_locks: false,
            pending_confirm: None,
            view: View::Changes,
            history: HistoryModel::default(),
            history_messages: None,
            changes_messages: None,
            diff: None,
            diff_messages: None,
            branches: BranchModel::default(),
            branch_messages: None,
            new_branch: String::new(),
            locks: LockModel::default(),
            lock_messages: None,
            lock_path: String::new(),
            live: LiveModel::default(),
            live_messages: None,
            updated: None,
        }
    }

    /// Subscribes to the remote's notifications for the instance, once its status has been read,
    /// replacing a subscription for an instance the tab showed before.
    fn subscribe(&mut self, ctx: &egui::Context) {
        if self.live.path.as_deref() == Some(self.path()) {
            return;
        }
        self.unsubscribe();
        let (sink, receiver) = repainting_channel(ctx);
        let path = self.path().to_owned();
        self.live.start(&path);
        match spawn_read(
            self.path(),
            LoreNotificationSubscribeArgs {}.into(),
            NotifyMessage::from_event,
            NotifyMessage::Failed,
            sink,
        ) {
            Ok(_) => self.live_messages = Some(receiver),
            Err(err) => self.live.apply(NotifyMessage::Failed(format!(
                "could not start a thread: {err}"
            ))),
        }
    }

    /// Ends the tab's subscription, if it has one.
    // ponytail: the library keeps one subscription per repository, so two tabs on instances of
    // the same repository share it, and closing one ends the other's live updates until it
    // switches instance; track subscribers per repository if that matters.
    fn unsubscribe(&mut self) {
        self.live_messages = None;
        let Some(path) = self.live.path.take() else {
            return;
        };
        if self.live.live == Live::On {
            let _ = spawn_read(
                &path,
                LoreNotificationUnsubscribeArgs {}.into(),
                |_| None::<()>,
                |_| (),
                |()| {},
            );
        }
        self.live.live = Live::Off;
    }

    /// The window gained focus.
    pub fn focused(&mut self) {
        self.live.focused();
    }

    /// Reads the status and every view the tab has read before, as Refresh does.
    fn refresh_views(&mut self, ctx: &egui::Context) {
        self.run(ctx, false, Change::None);
        if self.history.progress != Progress::Idle {
            self.load_history(ctx);
        }
        if self.branches.progress != Progress::Idle {
            self.load_branches(ctx);
        }
        if self.locks.progress != Progress::Idle {
            self.load_locks(ctx);
        }
    }

    /// Reads the locks the remote holds on the current branch, or on every branch while the
    /// current one is not yet known.
    fn load_locks(&mut self, ctx: &egui::Context) {
        if self.locks.progress == Progress::Running || self.path().is_empty() {
            return;
        }
        let branch = self
            .status
            .revision
            .as_ref()
            .map_or("", |revision| revision.branch.as_str());
        let args = LoreLockFileQueryArgs {
            branch: branch.into(),
            owner: "".into(),
            path: "".into(),
        };
        let (sink, receiver) = repainting_channel(ctx);
        self.locks.start();
        match spawn_read(
            self.path(),
            args.into(),
            LockMessage::from_event,
            |error| LockMessage::Complete(Err(error)),
            sink,
        ) {
            Ok(_) => self.lock_messages = Some(receiver),
            Err(err) => self.locks.apply(LockMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
    }

    /// Reads the instance's branches, replacing the last list read.
    fn load_branches(&mut self, ctx: &egui::Context) {
        if self.branches.progress == Progress::Running || self.path().is_empty() {
            return;
        }
        let (sink, receiver) = repainting_channel(ctx);
        self.branches.start();
        match spawn_read(
            self.path(),
            LoreBranchListArgs { archived: 0 }.into(),
            BranchMessage::from_event,
            |error| BranchMessage::Complete(Err(error)),
            sink,
        ) {
            Ok(_) => self.branch_messages = Some(receiver),
            Err(err) => self.branches.apply(BranchMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
    }

    /// Shows the diff of `path`, relative to the instance's root, from the `source` revision to
    /// the `target` one, beside the current view. An empty source is the current revision and an
    /// empty target the working tree, as for `lore file diff`.
    fn open_diff(&mut self, ctx: &egui::Context, path: &str, source: &str, target: &str) {
        let sides = match (source, target) {
            ("", "") => "current revision → working tree".to_owned(),
            (source, target) => format!("{} → {}", short(source), short(target)),
        };
        let args = LoreFileDiffArgs {
            paths: LoreArray::from_vec(vec![absolute_path(self.path(), path)]),
            source_revision: source.into(),
            target_revision: target.into(),
            diff3: 0,
            context_lines: DEFAULT_CONTEXT_LINES,
            ignore_whitespace_eol: 0,
            ignore_whitespace_inline: 0,
        };
        let (sink, receiver) = repainting_channel(ctx);
        let mut diff = DiffModel::start(path.to_owned(), sides);
        // A diff opened before is left to finish; its messages go nowhere.
        match spawn_read(
            self.path(),
            args.into(),
            DiffMessage::from_event,
            |error| DiffMessage::Complete(Err(error)),
            sink,
        ) {
            Ok(_) => self.diff_messages = Some(receiver),
            Err(err) => diff.apply(DiffMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
        self.diff = Some((self.view, diff));
    }

    /// Reads the branch's history, replacing the last one read.
    fn load_history(&mut self, ctx: &egui::Context) {
        if self.history.progress == Progress::Running || self.path().is_empty() {
            return;
        }
        let (sink, receiver) = repainting_channel(ctx);
        self.history.start();
        let spawned = spawn_read(
            self.path(),
            LoreRevisionHistoryArgs::default().into(),
            HistoryMessage::from_event,
            |error| HistoryMessage::Complete(Err(error)),
            sink,
        );
        match spawned {
            Ok(_) => self.history_messages = Some(receiver),
            Err(err) => self.history.apply(HistoryMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
    }

    /// Selects the revision at `index` and reads the files it changed against its parent.
    fn select_revision(&mut self, ctx: &egui::Context, index: usize) {
        let Some(revision) = self.history.revisions.get(index) else {
            return;
        };
        let args = LoreRevisionInfoArgs {
            revision: revision.revision.as_str().into(),
            delta: 1,
            metadata: 0,
        };
        let (sink, receiver) = repainting_channel(ctx);
        self.history.select(index);
        // A call on a revision selected before is left to finish; its messages go nowhere.
        let spawned = spawn_read(
            self.path(),
            args.into(),
            HistoryMessage::from_event,
            |error| HistoryMessage::Complete(Err(error)),
            sink,
        );
        match spawned {
            Ok(_) => self.changes_messages = Some(receiver),
            Err(err) => self
                .history
                .apply_changes(HistoryMessage::Complete(Err(format!(
                    "could not start a thread: {err}"
                )))),
        }
    }

    pub fn path(&self) -> &str {
        self.instance_path.trim()
    }

    /// The tab's label: the working tree's directory name.
    pub fn title(&self) -> String {
        let path = self.path().trim_end_matches(['/', '\\']);
        match path.rsplit(['/', '\\']).next() {
            Some(name) if !name.is_empty() => name.to_owned(),
            _ => "New instance".to_owned(),
        }
    }

    /// Reads the status, as Refresh does.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.run(ctx, false, Change::None);
    }

    /// Starts a call that makes `change`, then reads the status. One call runs at a time, so the
    /// client never runs two changes on the instance at once.
    fn run(&mut self, ctx: &egui::Context, scan: bool, change: Change) {
        if self.status.is_running() || self.instance_path.trim().is_empty() {
            return;
        }
        let (sink, receiver) = repainting_channel(ctx);
        self.committing = matches!(change, Change::Commit(_));
        self.changes_locks = matches!(change, Change::Lock(_) | Change::Unlock(_));
        self.moves_branch = !self.changes_locks
            && !matches!(
                change,
                Change::None
                    | Change::Stage(_)
                    | Change::Unstage(_)
                    | Change::Revert(_)
                    | Change::LogIn
                    | Change::LogOut
            );
        let request = StatusRequest {
            instance_path: self.instance_path.trim().to_owned(),
            scan,
            change,
        };
        self.status.start();
        match spawn_status(request, sink) {
            Ok(_) => self.messages = Some(receiver),
            Err(err) => self.status.apply(StatusMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
    }

    /// Folds the messages the running calls have sent since the last frame into the views.
    pub fn drain_messages(&mut self, ctx: &egui::Context) {
        let mut moved = false;
        let mut locked = false;
        let mut read = false;
        let finished = drain(&mut self.messages, |message| {
            // The status read follows only a change that succeeded.
            if message == StatusMessage::Begin {
                moved = self.moves_branch;
                locked = self.changes_locks;
                if self.committing {
                    self.commit_message.clear();
                }
            }
            read |= message == StatusMessage::Complete(Ok(()));
            self.status.apply(message);
        });
        if read {
            self.updated = Some(chrono::Local::now());
            self.subscribe(ctx);
        }
        if finished && self.status.is_running() {
            self.status.apply(StatusMessage::Complete(Err(
                "the call ended without reporting".to_owned(),
            )));
        }
        let finished = drain(&mut self.history_messages, |message| {
            self.history.apply(message);
        });
        if finished && self.history.progress == Progress::Running {
            self.history.apply(HistoryMessage::Complete(Err(
                "the call ended without reporting".to_owned(),
            )));
        }
        let finished = drain(&mut self.changes_messages, |message| {
            self.history.apply_changes(message);
        });
        if finished && self.history.changes_progress == Progress::Running {
            self.history.apply_changes(HistoryMessage::Complete(Err(
                "the call ended without reporting".to_owned(),
            )));
        }
        if let Some((_, diff)) = &mut self.diff {
            let finished = drain(&mut self.diff_messages, |message| diff.apply(message));
            if finished && diff.progress == Progress::Running {
                diff.apply(DiffMessage::Complete(Err(
                    "the call ended without reporting".to_owned(),
                )));
            }
        }
        let finished = drain(&mut self.branch_messages, |message| {
            self.branches.apply(message);
        });
        if finished && self.branches.progress == Progress::Running {
            self.branches.apply(BranchMessage::Complete(Err(
                "the call ended without reporting".to_owned(),
            )));
        }
        // A history or branch list read before the branch moved no longer shows it.
        if moved && self.history.progress != Progress::Idle {
            self.load_history(ctx);
        }
        if moved && self.branches.progress != Progress::Idle {
            self.load_branches(ctx);
        }
        let finished = drain(&mut self.lock_messages, |message| {
            self.locks.apply(message);
        });
        if finished && self.locks.progress == Progress::Running {
            self.locks.apply(LockMessage::Complete(Err(
                "the call ended without reporting".to_owned(),
            )));
        }
        // Switching branches changes which branch's locks the view lists.
        if (moved || locked) && self.locks.progress != Progress::Idle {
            self.load_locks(ctx);
        }
        drain(&mut self.live_messages, |message| self.live.apply(message));
        // A change the remote reported waits for the tab's own call to finish, so none is lost.
        if !self.status.is_running() && self.live.take_stale() {
            self.refresh_views(ctx);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Instance");
            let path = ui.add(
                egui::TextEdit::singleline(&mut self.instance_path)
                    .hint_text("path to a working tree")
                    .desired_width(ui.available_width() - 180.0),
            );
            let entered = path.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let idle = !self.status.is_running();
            let refresh = ui.add_enabled(idle, egui::Button::new("Refresh")).clicked();
            let scan = ui
                .add_enabled(idle, egui::Button::new("Scan"))
                .on_hover_text("Walk the working tree for changes made outside Lore")
                .clicked();
            if entered || refresh || scan {
                self.run(ui.ctx(), scan, Change::None);
                match self.view {
                    View::Changes => {}
                    View::History => self.load_history(ui.ctx()),
                    View::Branches => self.load_branches(ui.ctx()),
                    View::Locks => self.load_locks(ui.ctx()),
                }
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Changes, "Changes");
            let history = ui.selectable_value(&mut self.view, View::History, "History");
            if history.clicked() && self.history.progress == Progress::Idle {
                self.load_history(ui.ctx());
            }
            let branches = ui.selectable_value(&mut self.view, View::Branches, "Branches");
            if branches.clicked() && self.branches.progress == Progress::Idle {
                self.load_branches(ui.ctx());
            }
            let locks = ui.selectable_value(&mut self.view, View::Locks, "Locks");
            if locks.clicked() && self.locks.progress == Progress::Idle {
                self.load_locks(ui.ctx());
            }
            ui.separator();
            let idle = !self.status.is_running() && !self.path().is_empty();
            let mut change = None;
            if ui
                .add_enabled(idle, egui::Button::new("Sync"))
                .on_hover_text("Bring the working tree to the branch tip, merging if it diverged")
                .clicked()
            {
                change = Some(Change::Sync);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Push"))
                .on_hover_text("Push the branch's revisions to the remote")
                .clicked()
            {
                change = Some(Change::Push);
            }
            ui.separator();
            if let Some(user) = &self.status.user {
                ui.label(format!("Signed in as {user}"));
                if ui.add_enabled(idle, egui::Button::new("Log out")).clicked() {
                    change = Some(Change::LogOut);
                }
            } else {
                ui.weak("Not signed in");
                if ui
                    .add_enabled(idle, egui::Button::new("Log in"))
                    .on_hover_text("Sign in to the instance's remote in the browser")
                    .clicked()
                {
                    change = Some(Change::LogIn);
                }
            }
            if let Some(change) = change {
                self.run(ui.ctx(), false, change);
            }
        });
    }

    fn summary(&mut self, ui: &mut egui::Ui) {
        if let Some(updated) = self.updated {
            let updated = format!("Updated {}", updated.format("%H:%M:%S"));
            match &self.live.live {
                Live::On => {
                    ui.weak(format!("{updated} · live"))
                        .on_hover_text("Refreshes when someone pushes, branches or locks");
                }
                Live::Unavailable(reason) => {
                    ui.weak(format!("{updated} · refreshes when the window gains focus"))
                        .on_hover_text(format!("No live updates: {reason}"));
                }
                Live::Off | Live::Subscribing => {
                    ui.weak(updated);
                }
            }
        }
        match &self.status.progress {
            Progress::Idle => {
                ui.label("Enter the path of a repository instance and press Refresh.");
            }
            // The library has no way to cancel a call, so a running one shows only its progress.
            Progress::Running => match &self.status.note {
                Some((label, Some(fraction))) => {
                    ui.add(egui::ProgressBar::new(*fraction).text(label));
                }
                Some((label, None)) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(label);
                    });
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Working…");
                    });
                }
            },
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Done => {}
        }
        if let Some(revision) = &self.status.revision {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&revision.branch);
                ui.label(format!("revision {}", revision.revision_number));
                ui.weak(&revision.revision);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(revision.remote.label());
                if let Some(incoming) = &revision.pending_merge {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Merge in progress, incoming {}", short(incoming)),
                    );
                    let abort = ui
                        .add_enabled(!self.status.is_running(), egui::Button::new("Abort merge…"))
                        .on_hover_text("Discard the merge and return to the current revision");
                    if abort.clicked() {
                        self.pending_confirm = Some(Change::AbortMerge);
                    }
                    ui.weak("Resolve the conflicts, then commit to finish the merge.");
                }
            });
        }
    }

    /// The buttons over a section's table, and the change the one clicked asks for. With no row
    /// selected, Stage and Unstage cover the whole instance.
    fn section_actions(&mut self, ui: &mut egui::Ui, section: Section) -> Option<Change> {
        let idle = !self.status.is_running();
        let selected = self.status.selected_paths(section);
        let scope = |paths: &[String]| match paths.len() {
            0 => "all".to_owned(),
            n => format!("{n} selected"),
        };
        let all_or_selected = || match selected.is_empty() {
            true => vec![String::new()],
            false => selected.clone(),
        };
        match section {
            Section::Unstaged => {
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(format!("Stage {}", scope(&selected))),
                    )
                    .clicked()
                {
                    return Some(Change::Stage(all_or_selected()));
                }
                let revert = ui
                    .add_enabled(idle && !selected.is_empty(), egui::Button::new("Revert…"))
                    .on_hover_text("Discard the selected files' changes");
                if revert.clicked() {
                    self.pending_confirm = Some(Change::Revert(selected.clone()));
                }
            }
            Section::Staged => {
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(format!("Unstage {}", scope(&selected))),
                    )
                    .clicked()
                {
                    return Some(Change::Unstage(all_or_selected()));
                }
            }
            Section::Unmerged => {
                let conflicts = if selected.is_empty() {
                    self.status
                        .files(Section::Unmerged)
                        .iter()
                        .map(|file| file.path.clone())
                        .collect()
                } else {
                    selected.clone()
                };
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(format!("Keep mine ({})", scope(&selected))),
                    )
                    .on_hover_text("Resolve by keeping the current branch's version")
                    .clicked()
                {
                    return Some(Change::KeepMine(conflicts));
                }
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(format!("Keep theirs ({})", scope(&selected))),
                    )
                    .on_hover_text("Resolve by taking the incoming branch's version")
                    .clicked()
                {
                    return Some(Change::KeepTheirs(conflicts));
                }
            }
        }
        let diff = ui
            .add_enabled(selected.len() == 1, egui::Button::new("Diff"))
            .on_hover_text("Show the selected file's changes against the current revision")
            .on_disabled_hover_text("Select one file to see its changes");
        if diff.clicked() {
            self.open_diff(ui.ctx(), &selected[0], "", "");
        }
        let lock = ui
            .add_enabled(idle && !selected.is_empty(), egui::Button::new("Lock"))
            .on_hover_text("Lock the selected files so no one else can change them on this branch");
        if lock.clicked() {
            return Some(Change::Lock(selected));
        }
        None
    }

    fn files(&mut self, ui: &mut egui::Ui) {
        let sections: Vec<Section> = Section::ALL
            .into_iter()
            .filter(|section| !self.status.files(*section).is_empty())
            .collect();
        if sections.is_empty() {
            if self.status.progress == Progress::Done {
                ui.label("No changes.");
            }
            return;
        }
        // Each section gets an even share of the height, and its table draws only the rows in
        // view, so a status of any size costs the same to draw.
        let share = ui.available_height() / sections.len() as f32;
        let mut change = None;
        let mut clicked = None;
        for section in sections {
            ui.push_id(section.title(), |ui| {
                ui.horizontal(|ui| {
                    let count = self.status.files(section).len();
                    ui.strong(format!("{} ({count})", section.title()));
                    change = change.take().or(self.section_actions(ui, section));
                });
                let status = &self.status;
                let files = status.files(section);
                TableBuilder::new(ui)
                    .striped(true)
                    .sense(egui::Sense::click())
                    .max_scroll_height(share - 3.0 * ROW_HEIGHT)
                    .column(Column::exact(80.0))
                    .column(Column::remainder())
                    .column(Column::exact(90.0))
                    .header(ROW_HEIGHT, |mut header| {
                        header.col(|ui| {
                            ui.weak("Change");
                        });
                        header.col(|ui| {
                            ui.weak("Path");
                        });
                        header.col(|ui| {
                            ui.weak("Size");
                        });
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, files.len(), |mut row| {
                            let index = row.index();
                            let file = &files[index];
                            row.set_selected(status.is_selected(section, index));
                            row.col(|ui| {
                                ui.label(action_label(file.action));
                            });
                            row.col(|ui| match &file.from_path {
                                Some(from) => {
                                    ui.label(format!("{from} → {}", file.path));
                                }
                                None => {
                                    ui.label(&file.path);
                                }
                            });
                            row.col(|ui| {
                                ui.label(format!("{} B", file.size));
                            });
                            if row.response().clicked() {
                                clicked = Some((section, index));
                            }
                        });
                    });
            });
        }
        if let Some((section, index)) = clicked {
            self.status.toggle(section, index);
        }
        if let Some(change) = change {
            self.run(ui.ctx(), false, change);
        }
    }

    fn commit_box(&mut self, ui: &mut egui::Ui) {
        ui.label("Commit message");
        ui.add(
            egui::TextEdit::multiline(&mut self.commit_message)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        let staged = !self.status.files(Section::Staged).is_empty();
        let ready = staged && !self.commit_message.trim().is_empty() && !self.status.is_running();
        let commit = ui
            .add_enabled(ready, egui::Button::new("Commit"))
            .on_disabled_hover_text("Stage files and write a message to commit");
        if commit.clicked() {
            let message = self.commit_message.trim().to_owned();
            self.run(ui.ctx(), false, Change::Commit(message));
        }
    }

    /// Asks the user to confirm a revert, which discards changes the library cannot restore.
    fn confirm(&mut self, ctx: &egui::Context) {
        let (heading, detail, action) = match &self.pending_confirm {
            Some(Change::Revert(paths)) => (
                "Revert changes?",
                format!(
                    "Discard the changes to {} file(s)? This cannot be undone.",
                    paths.len()
                ),
                "Revert",
            ),
            Some(Change::AbortMerge) => (
                "Abort the merge?",
                "Discard the merge in progress, with every conflict resolved so far, and return \
                 the files to the current revision?"
                    .to_owned(),
                "Abort merge",
            ),
            _ => return,
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.heading(heading);
            ui.label(detail);
            ui.horizontal(|ui| {
                if ui.button(action).clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        if modal.should_close() && answer.is_none() {
            answer = Some(false);
        }
        if let Some(confirmed) = answer
            && let Some(change) = self.pending_confirm.take()
            && confirmed
        {
            self.run(ctx, false, change);
        }
    }
}

impl Instance {
    /// Draws the tab into `ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui);
            ui.add_space(4.0);
        });
        if matches!(&self.diff, Some((view, _)) if *view == self.view) {
            egui::Panel::right("diff")
                .resizable(true)
                .default_size(ui.available_width() * 0.55)
                .min_size(240.0)
                .show(ui, |ui| self.diff_panel(ui));
        }
        match self.view {
            View::Changes => {
                egui::Panel::bottom("commit").show(ui, |ui| {
                    ui.add_space(4.0);
                    self.commit_box(ui);
                    ui.add_space(4.0);
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    self.summary(ui);
                    ui.separator();
                    self.files(ui);
                });
            }
            View::History => {
                if self.history.selected.is_some() {
                    egui::Panel::bottom("revision")
                        .resizable(true)
                        .default_size(260.0)
                        .min_size(120.0)
                        .show(ui, |ui| self.revision_details(ui));
                }
                egui::CentralPanel::default().show(ui, |ui| self.revisions(ui));
            }
            View::Branches => {
                egui::CentralPanel::default().show(ui, |ui| self.branch_list(ui));
            }
            View::Locks => {
                egui::CentralPanel::default().show(ui, |ui| self.lock_list(ui));
            }
        }
        let ctx = ui.ctx().clone();
        self.confirm(&ctx);
    }

    /// The branch's revisions, newest first; a click selects one to show its changes.
    fn revisions(&mut self, ui: &mut egui::Ui) {
        match &self.history.progress {
            Progress::Running => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Reading history… {} revisions",
                        self.history.revisions.len()
                    ));
                });
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Idle | Progress::Done => {}
        }
        let mut clicked = None;
        let history = &self.history;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::exact(60.0))
            .column(Column::exact(130.0))
            .column(Column::exact(140.0))
            .column(Column::remainder())
            .header(ROW_HEIGHT, |mut header| {
                for title in ["#", "Date", "Created by", "Message"] {
                    header.col(|ui| {
                        ui.weak(title);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, history.revisions.len(), |mut row| {
                    let index = row.index();
                    let revision = &history.revisions[index];
                    row.set_selected(history.selected == Some(index));
                    row.col(|ui| {
                        ui.label(revision.number.to_string());
                    });
                    row.col(|ui| {
                        ui.label(revision.timestamp.map_or_else(String::new, format_time));
                    });
                    row.col(|ui| {
                        ui.label(&revision.created_by);
                    });
                    row.col(|ui| {
                        ui.label(revision.summary());
                    });
                    if row.response().clicked() {
                        clicked = Some(index);
                    }
                });
            });
        if let Some(index) = clicked {
            self.select_revision(ui.ctx(), index);
        }
    }

    /// The selected revision: its whole message, its parents and the files it changed.
    fn revision_details(&mut self, ui: &mut egui::Ui) {
        let Some(revision) = self.history.selected_revision() else {
            return;
        };
        let mut clicked = None;
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("Revision {}", revision.number));
            ui.weak(&revision.revision);
        });
        for parent in &revision.parents {
            ui.weak(format!("Parent {parent}"));
        }
        egui::ScrollArea::vertical()
            .id_salt("message")
            .max_height(80.0)
            .show(ui, |ui| {
                ui.label(&revision.message);
            });
        ui.separator();
        match &self.history.changes_progress {
            Progress::Running => {
                ui.spinner();
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Idle | Progress::Done => {}
        }
        let changes = &self.history.changes;
        // The first revision has no parent to show its files' changes against.
        let parent = revision.parents.first();
        let title = ui.strong(format!("Files changed ({})", changes.len()));
        if parent.is_some() {
            title.on_hover_text("Click a file to see its changes");
        }
        ui.push_id("changes", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .sense(egui::Sense::click())
                .column(Column::exact(80.0))
                .column(Column::remainder())
                .column(Column::exact(90.0))
                .body(|body| {
                    body.rows(ROW_HEIGHT, changes.len(), |mut row| {
                        let change = &changes[row.index()];
                        row.col(|ui| {
                            ui.label(action_label(change.action));
                        });
                        row.col(|ui| match &change.from_path {
                            Some(from) => {
                                ui.label(format!("{from} → {}", change.path));
                            }
                            None => {
                                ui.label(&change.path);
                            }
                        });
                        row.col(|ui| {
                            ui.label(format!("{} B", change.size));
                        });
                        if row.response().clicked() {
                            clicked = Some(change.path.clone());
                        }
                    });
                });
        });
        if let (Some(path), Some(parent)) = (clicked, parent.cloned()) {
            let target = revision.revision.clone();
            self.open_diff(ui.ctx(), &path, &parent, &target);
        }
    }

    /// The instance's branches, with creating one, switching to one and merging one into the
    /// current branch.
    fn branch_list(&mut self, ui: &mut egui::Ui) {
        let idle = !self.status.is_running();
        let mut change = None;
        ui.horizontal(|ui| {
            let name = ui.add(
                egui::TextEdit::singleline(&mut self.new_branch)
                    .hint_text("new branch name")
                    .desired_width(220.0),
            );
            let entered = name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ready = idle && !self.new_branch.trim().is_empty();
            let create = ui
                .add_enabled(ready, egui::Button::new("Create"))
                .on_hover_text("Create a branch at the current revision and switch to it");
            if ready && (entered || create.clicked()) {
                change = Some(Change::CreateBranch(self.new_branch.trim().to_owned()));
            }
            ui.separator();
            let selected = self.branches.selected_row().filter(|row| !row.current);
            let name = selected.map(|row| row.name.clone());
            let switch = ui
                .add_enabled(idle && name.is_some(), egui::Button::new("Switch"))
                .on_hover_text("Bring the working tree to the selected branch")
                .on_disabled_hover_text("Select a branch other than the current one");
            if let (true, Some(name)) = (switch.clicked(), &name) {
                change = Some(Change::Switch(name.clone()));
            }
            let merge = ui
                .add_enabled(
                    idle && name.is_some(),
                    egui::Button::new("Merge into current"),
                )
                .on_hover_text(
                    "Merge the selected branch into the current one; conflicts appear in Changes",
                )
                .on_disabled_hover_text("Select a branch other than the current one");
            if let (true, Some(name)) = (merge.clicked(), name) {
                change = Some(Change::Merge(name));
            }
        });
        match &self.status.progress {
            Progress::Running => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.status.note.as_ref().map_or("Working…", |n| &n.0));
                });
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Idle | Progress::Done => {}
        }
        match &self.branches.progress {
            Progress::Running => {
                ui.spinner();
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Idle | Progress::Done => {}
        }
        ui.separator();
        let mut clicked = None;
        let branches = &self.branches;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::exact(20.0))
            .column(Column::remainder())
            .column(Column::exact(70.0))
            .column(Column::exact(120.0))
            .header(ROW_HEIGHT, |mut header| {
                for title in ["", "Branch", "Where", "Latest"] {
                    header.col(|ui| {
                        ui.weak(title);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, branches.rows.len(), |mut row| {
                    let index = row.index();
                    let branch = &branches.rows[index];
                    row.set_selected(branches.selected == Some(index));
                    row.col(|ui| {
                        if branch.current {
                            ui.strong("●").on_hover_text("The current branch");
                        }
                    });
                    row.col(|ui| {
                        if branch.current {
                            ui.strong(&branch.name);
                        } else {
                            ui.label(&branch.name);
                        }
                    });
                    row.col(|ui| {
                        ui.label(if branch.remote { "remote" } else { "local" });
                    });
                    row.col(|ui| {
                        ui.weak(short(&branch.latest));
                    });
                    if row.response().clicked() {
                        clicked = Some(index);
                    }
                });
            });
        if let Some(index) = clicked {
            self.branches.selected = Some(index);
        }
        if let Some(change) = change {
            if matches!(change, Change::CreateBranch(_)) {
                self.new_branch.clear();
            }
            self.run(ui.ctx(), false, change);
        }
    }

    /// The locks the remote holds on the current branch, with locking a path and releasing the
    /// selected locks.
    fn lock_list(&mut self, ui: &mut egui::Ui) {
        let idle = !self.status.is_running();
        let mut change = None;
        ui.horizontal(|ui| {
            let path = ui.add(
                egui::TextEdit::singleline(&mut self.lock_path)
                    .hint_text("path to lock, from the instance's root")
                    .desired_width(280.0),
            );
            let entered = path.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ready = idle && !self.lock_path.trim().is_empty();
            let lock = ui
                .add_enabled(ready, egui::Button::new("Lock"))
                .on_hover_text("Lock the file so no one else can change it on this branch");
            if ready && (entered || lock.clicked()) {
                change = Some(Change::Lock(vec![self.lock_path.trim().to_owned()]));
                self.lock_path.clear();
            }
            ui.separator();
            let selected = self.locks.selected_paths();
            let unlock = ui
                .add_enabled(
                    idle && !selected.is_empty(),
                    egui::Button::new(format!("Unlock {} selected", selected.len())),
                )
                .on_hover_text(
                    "Release the selected locks; the remote refuses locks you don't hold",
                )
                .on_disabled_hover_text("Select locks to release");
            if unlock.clicked() {
                change = Some(Change::Unlock(selected));
            }
        });
        match &self.status.progress {
            Progress::Running => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Working…");
                });
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Idle | Progress::Done => {}
        }
        match &self.locks.progress {
            Progress::Running => {
                ui.spinner();
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Done if self.locks.rows.is_empty() => {
                ui.label("No files are locked on this branch.");
            }
            Progress::Idle | Progress::Done => {}
        }
        ui.separator();
        let mut clicked = None;
        let locks = &self.locks;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::remainder())
            .column(Column::exact(180.0))
            .column(Column::exact(130.0))
            .header(ROW_HEIGHT, |mut header| {
                for title in ["Path", "Held by", "Since"] {
                    header.col(|ui| {
                        ui.weak(title);
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, locks.rows.len(), |mut row| {
                    let index = row.index();
                    let lock = &locks.rows[index];
                    row.set_selected(locks.is_selected(index));
                    row.col(|ui| {
                        ui.label(&lock.path);
                    });
                    row.col(|ui| {
                        ui.label(&lock.owner);
                    });
                    row.col(|ui| {
                        ui.label(format_time(lock.locked_at));
                    });
                    if row.response().clicked() {
                        clicked = Some(index);
                    }
                });
            });
        if let Some(index) = clicked {
            self.locks.toggle(index);
        }
        if let Some(change) = change {
            self.run(ui.ctx(), false, change);
        }
    }

    /// The open diff, side by side: the old file's lines on the left, the new file's on the right.
    fn diff_panel(&mut self, ui: &mut egui::Ui) {
        let Some((_, diff)) = &self.diff else {
            return;
        };
        let mut close = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            close = ui
                .small_button("×")
                .on_hover_text("Close the diff")
                .clicked();
            ui.strong(&diff.path);
        });
        ui.weak(&diff.sides);
        match &diff.progress {
            Progress::Running => {
                ui.spinner();
            }
            Progress::Failed(message) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            Progress::Done if diff.rows.is_empty() => {
                ui.label("No differences.");
            }
            Progress::Idle | Progress::Done => {}
        }
        if diff.cut {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "The diff is cut at {} MB; the rest is not shown.",
                    DIFF_CAP >> 20
                ),
            );
        }
        ui.separator();
        let removed = ui.visuals().error_fg_color;
        let added = egui::Color32::from_rgb(0x3f, 0xa3, 0x4d);
        let text_width = ((ui.available_width() - 2.0 * 44.0) / 2.0).max(80.0);
        let rows = &diff.rows;
        TableBuilder::new(ui)
            .striped(false)
            .column(Column::exact(40.0))
            .column(Column::exact(text_width).clip(true))
            .column(Column::exact(40.0))
            .column(Column::remainder().clip(true))
            .body(|body| {
                body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                    let line = &rows[row.index()];
                    if matches!(line.kind, RowKind::Hunk | RowKind::Note) {
                        row.col(|_| {});
                        row.col(|ui| {
                            ui.weak(egui::RichText::new(&line.text).monospace());
                        });
                        row.col(|_| {});
                        row.col(|_| {});
                        return;
                    }
                    for (side, colour) in [(&line.old, removed), (&line.new, added)] {
                        let changed = line.kind != RowKind::Context;
                        row.col(|ui| {
                            if let Some((number, _)) = side {
                                ui.weak(number.to_string());
                            }
                        });
                        row.col(|ui| {
                            if let Some((_, text)) = side {
                                let text = egui::RichText::new(text).monospace();
                                ui.label(if changed { text.color(colour) } else { text });
                            }
                        });
                    }
                });
            });
        if close {
            self.diff = None;
            self.diff_messages = None;
        }
    }
}

/// The start of a revision hash, enough to tell revisions apart in a header.
fn short(revision: &str) -> &str {
    &revision[..revision.len().min(12)]
}

/// A commit time, in milliseconds since the Unix epoch, as local date and time.
fn format_time(millis: u64) -> String {
    chrono::DateTime::from_timestamp_millis(millis as i64).map_or_else(String::new, |time| {
        time.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    })
}
