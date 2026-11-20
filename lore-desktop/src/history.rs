// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The history view's model: a branch's revisions, and the files one of them changed, folded from
//! the events of `revision history` and `revision info`.

use lore::interface::LoreEvent;
use lore::interface::LoreFileAction;
use lore::interface::LoreMetadata;
use lore::interface::metadata;

use crate::status::Progress;

/// One revision of the history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RevisionRow {
    pub number: u64,
    pub revision: String,
    /// The direct parent, and the other parent of a merge.
    pub parents: Vec<String>,
    pub message: String,
    /// When the revision was committed, in milliseconds since the Unix epoch.
    pub timestamp: Option<u64>,
    pub created_by: String,
}

impl RevisionRow {
    /// The first line of the message, for the list.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }
}

/// One file a revision changed against its parent.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRow {
    pub path: String,
    /// The previous path of a moved file.
    pub from_path: Option<String>,
    pub action: LoreFileAction,
    pub size: u64,
}

/// What a history or revision info call reports, copied out of its events.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryMessage {
    Revision(RevisionRow),
    /// A metadata entry of the revision reported last.
    Message(String),
    Timestamp(u64),
    CreatedBy(String),
    Change(ChangeRow),
    /// The call finished: `Err` carries the library's error message.
    Complete(Result<(), String>),
}

impl HistoryMessage {
    /// The message for `event`, or `None` for an event the history view does not show.
    pub fn from_event(event: &LoreEvent) -> Option<Self> {
        match event {
            LoreEvent::RevisionHistoryEntry(data) => Some(Self::Revision(RevisionRow {
                number: data.revision_number,
                revision: data.revision.to_string(),
                parents: data
                    .parent
                    .iter()
                    .filter(|parent| !parent.is_zero())
                    .map(ToString::to_string)
                    .collect(),
                ..Default::default()
            })),
            LoreEvent::Metadata(data) => match (data.key.as_str(), &data.value) {
                (metadata::MESSAGE, LoreMetadata::String(value)) => {
                    Some(Self::Message(value.as_str().to_owned()))
                }
                (metadata::TIMESTAMP, LoreMetadata::Numeric(value)) => {
                    Some(Self::Timestamp(*value))
                }
                (metadata::CREATED_BY, LoreMetadata::String(value)) => {
                    Some(Self::CreatedBy(value.as_str().to_owned()))
                }
                _ => None,
            },
            // A directory's entry repeats what its files say.
            LoreEvent::RevisionInfoDelta(data) if data.flag_file != 0 => {
                Some(Self::Change(ChangeRow {
                    path: data.path.as_str().to_owned(),
                    from_path: (!data.from_path.is_empty())
                        .then(|| data.from_path.as_str().to_owned()),
                    action: data.action,
                    size: data.size,
                }))
            }
            LoreEvent::Complete(data) => Some(Self::Complete(if data.status == 0 {
                Ok(())
            } else {
                Err(data.error.message.as_str().to_owned())
            })),
            _ => None,
        }
    }
}

/// The history view's state: the revisions of the last history call, and the changes of the
/// selected one.
#[derive(Debug, Default)]
pub struct HistoryModel {
    pub revisions: Vec<RevisionRow>,
    pub progress: Progress,
    /// The index of the revision whose changes are shown.
    pub selected: Option<usize>,
    pub changes: Vec<ChangeRow>,
    pub changes_progress: Progress,
}

impl HistoryModel {
    /// Clears the last history, for a new call.
    pub fn start(&mut self) {
        self.revisions.clear();
        self.selected = None;
        self.changes.clear();
        self.changes_progress = Progress::Idle;
        self.progress = Progress::Running;
    }

    /// Folds one message of the history call into the state.
    pub fn apply(&mut self, message: HistoryMessage) {
        let last = self.revisions.last_mut();
        match (message, last) {
            (HistoryMessage::Revision(row), _) => self.revisions.push(row),
            (HistoryMessage::Message(text), Some(row)) => row.message = text,
            (HistoryMessage::Timestamp(time), Some(row)) => row.timestamp = Some(time),
            (HistoryMessage::CreatedBy(user), Some(row)) => row.created_by = user,
            (HistoryMessage::Complete(Ok(())), _) => self.progress = Progress::Done,
            (HistoryMessage::Complete(Err(error)), _) => self.progress = Progress::Failed(error),
            // Metadata before any revision, and changes, belong to no row of the list.
            _ => {}
        }
    }

    /// Selects the revision at `index`, for its changes to be read.
    pub fn select(&mut self, index: usize) {
        self.selected = Some(index);
        self.changes.clear();
        self.changes_progress = Progress::Running;
    }

    /// Folds one message of the selected revision's info call into the state.
    pub fn apply_changes(&mut self, message: HistoryMessage) {
        match message {
            HistoryMessage::Change(row) => self.changes.push(row),
            HistoryMessage::Complete(Ok(())) => {
                self.changes.sort_unstable_by(|a, b| a.path.cmp(&b.path));
                self.changes_progress = Progress::Done;
            }
            HistoryMessage::Complete(Err(error)) => {
                self.changes_progress = Progress::Failed(error);
            }
            _ => {}
        }
    }

    pub fn selected_revision(&self) -> Option<&RevisionRow> {
        self.revisions.get(self.selected?)
    }
}
