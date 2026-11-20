// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The branches view's model: the instance's branches, folded from the events of `branch list`.

use lore::interface::LoreBranchLocation;
use lore::interface::LoreEvent;

use crate::status::Progress;

/// One branch, as one location holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    pub name: String,
    /// The server holds this branch, rather than the instance.
    pub remote: bool,
    pub current: bool,
    /// The revision the branch points at.
    pub latest: String,
}

/// What a branch list call reports, copied out of its events.
#[derive(Debug, Clone, PartialEq)]
pub enum BranchMessage {
    Branch(BranchRow),
    /// The call finished: `Err` carries the library's error message.
    Complete(Result<(), String>),
}

impl BranchMessage {
    pub fn from_event(event: &LoreEvent) -> Option<Self> {
        match event {
            LoreEvent::BranchListEntry(data) => Some(Self::Branch(BranchRow {
                name: data.name.as_str().to_owned(),
                remote: data.location == LoreBranchLocation::Remote,
                current: data.is_current != 0,
                latest: data.latest.to_string(),
            })),
            LoreEvent::Complete(data) => Some(Self::Complete(if data.status == 0 {
                Ok(())
            } else {
                Err(data.error.message.as_str().to_owned())
            })),
            _ => None,
        }
    }
}

/// The branches view's state.
#[derive(Debug, Default)]
pub struct BranchModel {
    pub rows: Vec<BranchRow>,
    pub progress: Progress,
    /// The index of the selected row.
    pub selected: Option<usize>,
}

impl BranchModel {
    /// Clears the last list, for a new call.
    pub fn start(&mut self) {
        self.rows.clear();
        self.selected = None;
        self.progress = Progress::Running;
    }

    pub fn apply(&mut self, message: BranchMessage) {
        match message {
            BranchMessage::Branch(row) => self.rows.push(row),
            BranchMessage::Complete(Ok(())) => {
                // By name, with the instance's own copy of a branch before the server's.
                self.rows
                    .sort_by(|a, b| a.name.cmp(&b.name).then(a.remote.cmp(&b.remote)));
                self.progress = Progress::Done;
            }
            BranchMessage::Complete(Err(error)) => self.progress = Progress::Failed(error),
        }
    }

    pub fn selected_row(&self) -> Option<&BranchRow> {
        self.rows.get(self.selected?)
    }
}
