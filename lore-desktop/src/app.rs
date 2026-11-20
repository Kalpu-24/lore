// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The application window: a tab per open repository instance.

use eframe::egui;

use crate::instance::Instance;

/// The key the open instances are stored under, in eframe's per-user storage.
const OPEN_INSTANCES: &str = "open_instances";

/// The desktop client's window.
#[lore_macro::test_pub]
pub struct LoreDesktop {
    tabs: Vec<Instance>,
    active: usize,
}

impl LoreDesktop {
    /// A window on the instances open when the client last closed, and on `opened`, the path
    /// given on the command line, which becomes the active tab.
    pub fn new(remembered: Vec<String>, opened: Option<String>) -> Self {
        let mut paths = remembered;
        paths.retain(|path| !path.trim().is_empty());
        let mut active = 0;
        if let Some(opened) = opened {
            active = if let Some(index) = paths.iter().position(|path| *path == opened) {
                index
            } else {
                paths.push(opened);
                paths.len() - 1
            };
        }
        let mut tabs: Vec<Instance> = paths.into_iter().map(Instance::new).collect();
        if tabs.is_empty() {
            tabs.push(Instance::new(String::new()));
        }
        Self { tabs, active }
    }

    /// Reads every open instance's status.
    pub fn refresh_all(&mut self, ctx: &egui::Context) {
        for tab in &mut self.tabs {
            tab.refresh(ctx);
        }
    }

    /// The paths of the open instances, as they are remembered.
    pub fn open_paths(&self) -> Vec<String> {
        self.tabs
            .iter()
            .map(|tab| tab.path().to_owned())
            .filter(|path| !path.is_empty())
            .collect()
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        ui.horizontal_wrapped(|ui| {
            for (index, tab) in self.tabs.iter().enumerate() {
                let title = ui
                    .selectable_label(index == self.active, tab.title())
                    .on_hover_text(tab.path());
                if title.clicked() {
                    self.active = index;
                }
                if ui.small_button("×").on_hover_text("Close").clicked() {
                    close = Some(index);
                }
                ui.separator();
            }
            if ui
                .button("+")
                .on_hover_text("Open another instance")
                .clicked()
            {
                self.tabs.push(Instance::new(String::new()));
                self.active = self.tabs.len() - 1;
            }
        });
        if let Some(index) = close {
            self.close(index);
        }
    }

    /// Closes the tab at `index`. A call it was running finishes on its own; nothing is left to
    /// show its result.
    pub fn close(&mut self, index: usize) {
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.tabs.push(Instance::new(String::new()));
        }
        if self.active > index || self.active == self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    /// Draws one frame of the window into `ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // Every tab takes its messages, so a call in a tab out of view still finishes.
        let ctx = ui.ctx().clone();
        for tab in &mut self.tabs {
            tab.drain_messages(&ctx);
        }
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.add_space(2.0);
            self.tab_bar(ui);
            ui.add_space(2.0);
        });
        self.tabs[self.active].show(ui);
    }
}

impl eframe::App for LoreDesktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, OPEN_INSTANCES, &self.open_paths());
    }
}

/// The instances open when the client last closed.
pub fn remembered_instances(storage: Option<&dyn eframe::Storage>) -> Vec<String> {
    storage
        .and_then(|storage| eframe::get_value(storage, OPEN_INSTANCES))
        .unwrap_or_default()
}
