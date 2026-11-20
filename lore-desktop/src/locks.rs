// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The locks view's model: the file locks the remote holds on a branch, folded from the events of
//! `lock file query`.

use std::collections::BTreeSet;

use lore::interface::LoreEvent;

use crate::status::Progress;

/// One file lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockRow {
    /// The locked path, relative to the instance's root.
    pub path: String,
    /// The identity of the user holding the lock.
    pub owner: String,
    /// When the lock was taken, in milliseconds since the Unix epoch.
    pub locked_at: u64,
}

/// What a lock query reports, copied out of its events.
#[derive(Debug, Clone, PartialEq)]
pub enum LockMessage {
    Lock(LockRow),
    /// The call finished: `Err` carries the library's error message.
    Complete(Result<(), String>),
}

impl LockMessage {
    pub fn from_event(event: &LoreEvent) -> Option<Self> {
        match event {
            LoreEvent::LockFileQuery(data) => Some(Self::Lock(LockRow {
                path: data.path.as_str().to_owned(),
                owner: data.owner.as_str().to_owned(),
                locked_at: data.locked_at,
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

/// The locks view's state: the last query's locks, and the rows selected among them.
#[derive(Debug, Default)]
pub struct LockModel {
    pub rows: Vec<LockRow>,
    pub progress: Progress,
    /// Indexes into `rows`, which change only on [`LockModel::start`].
    selected: BTreeSet<usize>,
}

impl LockModel {
    /// Clears the last query, for a new one.
    pub fn start(&mut self) {
        self.rows.clear();
        self.selected.clear();
        self.progress = Progress::Running;
    }

    pub fn apply(&mut self, message: LockMessage) {
        match message {
            LockMessage::Lock(row) => self.rows.push(row),
            LockMessage::Complete(Ok(())) => {
                self.rows.sort_unstable_by(|a, b| a.path.cmp(&b.path));
                self.progress = Progress::Done;
            }
            LockMessage::Complete(Err(error)) => self.progress = Progress::Failed(error),
        }
    }

    /// Selects the row at `index`, or clears it if it was selected.
    pub fn toggle(&mut self, index: usize) {
        if !self.selected.remove(&index) {
            self.selected.insert(index);
        }
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// The paths of the selected locks, in display order.
    pub fn selected_paths(&self) -> Vec<String> {
        self.selected
            .iter()
            .map(|&index| self.rows[index].path.clone())
            .collect()
    }
}
