// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! The diff view's model: one file's unified diff, from `file diff`, laid out side by side.

use lore::interface::LoreEvent;

use crate::status::Progress;

/// The most patch text kept for one file. A larger diff is cut here, so the client never holds
/// more than this of any file's content.
pub const DIFF_CAP: usize = 1 << 20;

/// What a side-by-side row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// The same line on both sides.
    Context,
    /// A line only the old side holds.
    Removed,
    /// A line only the new side holds.
    Added,
    /// A line the new side holds in place of the old side's.
    Changed,
    /// A hunk's header, naming where it starts.
    Hunk,
    /// Text that is no line of either file, such as a move or "Binary files differ".
    Note,
}

/// One row of the side-by-side diff: a line of the old file, of the new file, or of both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub kind: RowKind,
    /// The old file's line number and text.
    pub old: Option<(u32, String)>,
    /// The new file's line number and text.
    pub new: Option<(u32, String)>,
    /// The text of a hunk header or note.
    pub text: String,
}

impl DiffRow {
    fn note(kind: RowKind, text: &str) -> Self {
        Self {
            kind,
            old: None,
            new: None,
            text: text.to_owned(),
        }
    }
}

/// Lays a unified diff out side by side. Within a hunk, a run of removed lines and the run of
/// added lines after it pair up row by row as changed lines; what is left of the longer run
/// stands alone.
pub fn side_by_side(patch: &str) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut removed: Vec<(u32, String)> = Vec::new();
    let mut added: Vec<(u32, String)> = Vec::new();
    let (mut old_line, mut new_line) = (0, 0);
    let mut in_hunk = false;
    let flush = |rows: &mut Vec<DiffRow>, removed: &mut Vec<_>, added: &mut Vec<_>| {
        let count = removed.len().max(added.len());
        let mut removed = removed.drain(..);
        let mut added = added.drain(..);
        for _ in 0..count {
            let (old, new) = (removed.next(), added.next());
            let kind = match (&old, &new) {
                (Some(_), Some(_)) => RowKind::Changed,
                (Some(_), None) => RowKind::Removed,
                _ => RowKind::Added,
            };
            rows.push(DiffRow {
                kind,
                old,
                new,
                text: String::new(),
            });
        }
    };
    for line in patch.lines() {
        if let Some((old_start, new_start)) = hunk_starts(line) {
            flush(&mut rows, &mut removed, &mut added);
            (old_line, new_line) = (old_start, new_start);
            in_hunk = true;
            rows.push(DiffRow::note(RowKind::Hunk, line));
            continue;
        }
        if !in_hunk {
            // The `---` and `+++` headers name the two sides, which the view names itself.
            if !line.starts_with("--- ") && !line.starts_with("+++ ") {
                rows.push(DiffRow::note(RowKind::Note, line));
            }
            continue;
        }
        match line.split_at_checked(1) {
            Some(("-", text)) => {
                removed.push((old_line, text.to_owned()));
                old_line += 1;
            }
            Some(("+", text)) => {
                added.push((new_line, text.to_owned()));
                new_line += 1;
            }
            Some((" ", text)) => {
                flush(&mut rows, &mut removed, &mut added);
                rows.push(DiffRow {
                    kind: RowKind::Context,
                    old: Some((old_line, text.to_owned())),
                    new: Some((new_line, text.to_owned())),
                    text: String::new(),
                });
                old_line += 1;
                new_line += 1;
            }
            // "\ No newline at end of file" qualifies the line before it.
            Some(("\\", _)) => {}
            _ => {
                flush(&mut rows, &mut removed, &mut added);
                rows.push(DiffRow::note(RowKind::Note, line));
            }
        }
    }
    flush(&mut rows, &mut removed, &mut added);
    rows
}

/// The first old and new line numbers of a hunk header such as `@@ -3,7 +3,8 @@`.
fn hunk_starts(line: &str) -> Option<(u32, u32)> {
    let ranges = line.strip_prefix("@@ -")?.split(" @@").next()?;
    let (old, new) = ranges.split_once(" +")?;
    let start = |range: &str| range.split(',').next()?.parse().ok();
    Some((start(old)?, start(new)?))
}

/// What a diff call reports, copied out of its events.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffMessage {
    /// A file's patch, cut at [`DIFF_CAP`]; `true` when it was cut.
    Patch(String, bool),
    /// The call finished: `Err` carries the library's error message.
    Complete(Result<(), String>),
}

impl DiffMessage {
    pub fn from_event(event: &LoreEvent) -> Option<Self> {
        match event {
            LoreEvent::FileDiff(data) => {
                let patch = data.patch.as_str();
                let cut = patch.len() > DIFF_CAP;
                let end = patch.floor_char_boundary(DIFF_CAP.min(patch.len()));
                Some(Self::Patch(patch[..end].to_owned(), cut))
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

/// The diff view's state: one file's diff between two sides.
#[derive(Debug, Default)]
pub struct DiffModel {
    pub path: String,
    /// What the two sides are, for the view's header.
    pub sides: String,
    pub rows: Vec<DiffRow>,
    /// The patch was cut at [`DIFF_CAP`].
    pub cut: bool,
    pub progress: Progress,
}

impl DiffModel {
    /// A diff of `path` between the sides `sides` names, to be filled by its call.
    pub fn start(path: String, sides: String) -> Self {
        Self {
            path,
            sides,
            progress: Progress::Running,
            ..Self::default()
        }
    }

    pub fn apply(&mut self, message: DiffMessage) {
        match message {
            DiffMessage::Patch(patch, cut) => {
                self.rows.extend(side_by_side(&patch));
                self.cut |= cut;
            }
            DiffMessage::Complete(Ok(())) => self.progress = Progress::Done,
            DiffMessage::Complete(Err(error)) => self.progress = Progress::Failed(error),
        }
    }
}
