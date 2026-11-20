// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The application window.

use std::sync::mpsc;

use eframe::egui;
use egui_extras::Column;
use egui_extras::TableBuilder;

use crate::status::Progress;
use crate::status::Section;
use crate::status::StatusMessage;
use crate::status::StatusModel;
use crate::status::action_label;
use crate::worker::Change;
use crate::worker::StatusRequest;
use crate::worker::spawn_status;

const ROW_HEIGHT: f32 = 18.0;

/// The desktop client's window: one repository instance and its status.
#[lore_macro::test_pub]
pub struct LoreDesktop {
    instance_path: String,
    status: StatusModel,
    /// Messages of the running call, sent from the library's threads.
    messages: Option<mpsc::Receiver<StatusMessage>>,
    commit_message: String,
    /// The running call commits: its message is cleared once the commit succeeds.
    committing: bool,
    /// Paths waiting for the user to confirm that their changes are to be discarded.
    pending_revert: Option<Vec<String>>,
}

impl LoreDesktop {
    pub fn new(instance_path: Option<String>) -> Self {
        Self {
            instance_path: instance_path.unwrap_or_default(),
            status: StatusModel::default(),
            messages: None,
            commit_message: String::new(),
            committing: false,
            pending_revert: None,
        }
    }

    /// Starts a call that makes `change`, then reads the status. One call runs at a time, so the
    /// client never runs two changes on the instance at once.
    fn run(&mut self, ctx: &egui::Context, scan: bool, change: Change) {
        if self.status.is_running() || self.instance_path.trim().is_empty() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        self.committing = matches!(change, Change::Commit(_));
        let request = StatusRequest {
            instance_path: self.instance_path.trim().to_owned(),
            scan,
            change,
        };
        self.status.start();
        let spawned = spawn_status(request, move |message| {
            // The window may have closed; nothing is left to show the message then.
            let _ = sender.send(message);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => self.messages = Some(receiver),
            Err(err) => self.status.apply(StatusMessage::Complete(Err(format!(
                "could not start a thread: {err}"
            )))),
        }
    }

    fn drain_messages(&mut self) {
        let Some(receiver) = &self.messages else {
            return;
        };
        let mut finished = false;
        loop {
            match receiver.try_recv() {
                Ok(message) => {
                    finished |= matches!(message, StatusMessage::Complete(_));
                    // The status read follows only a change that succeeded.
                    if message == StatusMessage::Begin && self.committing {
                        self.commit_message.clear();
                    }
                    self.status.apply(message);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.status.is_running() {
                        self.status.apply(StatusMessage::Complete(Err(
                            "the status call ended without reporting".to_owned(),
                        )));
                    }
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.messages = None;
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
            }
        });
    }

    fn summary(&self, ui: &mut egui::Ui) {
        match &self.status.progress {
            Progress::Idle => {
                ui.label("Enter the path of a repository instance and press Refresh.");
            }
            Progress::Running => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Reading status…");
                });
            }
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
                        format!("Merge in progress, incoming {incoming}"),
                    );
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
                    self.pending_revert = Some(selected.clone());
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
            // Resolving conflicts belongs to the merge view.
            Section::Unmerged => {}
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
    fn confirm_revert(&mut self, ctx: &egui::Context) {
        let Some(paths) = &self.pending_revert else {
            return;
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm-revert")).show(ctx, |ui| {
            ui.heading("Revert changes?");
            ui.label(format!(
                "Discard the changes to {} file(s)? This cannot be undone.",
                paths.len()
            ));
            ui.horizontal(|ui| {
                if ui.button("Revert").clicked() {
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
            && let Some(paths) = self.pending_revert.take()
            && confirmed
        {
            self.run(ctx, false, Change::Revert(paths));
        }
    }
}

impl eframe::App for LoreDesktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl LoreDesktop {
    /// Draws one frame of the window into `ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.drain_messages();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui);
            ui.add_space(4.0);
        });
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
        let ctx = ui.ctx().clone();
        self.confirm_revert(&ctx);
    }
}
