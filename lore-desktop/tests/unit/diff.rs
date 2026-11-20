// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore::interface::LoreEvent;
use lore::interface::LoreFileAction;
use lore_desktop::diff::DIFF_CAP;
use lore_desktop::diff::DiffMessage;
use lore_desktop::diff::DiffModel;
use lore_desktop::diff::RowKind;
use lore_desktop::diff::side_by_side;
use lore_desktop::status::Progress;
use lore_revision::file::diff::LoreFileDiffEventData;

/// A side's line: its number and text.
type Line = Option<(u32, String)>;

/// Each row as (kind, old line, new line).
fn layout(patch: &str) -> Vec<(RowKind, Line, Line)> {
    side_by_side(patch)
        .into_iter()
        .map(|row| (row.kind, row.old, row.new))
        .collect()
}

fn line(number: u32, text: &str) -> Line {
    Some((number, text.to_owned()))
}

#[test]
fn removed_and_added_runs_pair_up_as_changed_lines() {
    let patch = "\
--- a.txt
+++ a.txt
@@ -3,4 +3,4 @@
 keep
-old one
-old two
-old three
+new one
+new two
 tail
";
    assert_eq!(
        layout(patch),
        [
            (RowKind::Hunk, None, None),
            (RowKind::Context, line(3, "keep"), line(3, "keep")),
            (RowKind::Changed, line(4, "old one"), line(4, "new one")),
            (RowKind::Changed, line(5, "old two"), line(5, "new two")),
            // What is left of the longer run stands alone.
            (RowKind::Removed, line(6, "old three"), None),
            (RowKind::Context, line(7, "tail"), line(6, "tail")),
        ]
    );
}

#[test]
fn additions_alone_and_a_second_hunk_restart_the_numbers() {
    let patch = "\
@@ -1,1 +1,2 @@
 a
+b
@@ -10,2 +11,1 @@
-x
 y
\\ No newline at end of file
";
    assert_eq!(
        layout(patch),
        [
            (RowKind::Hunk, None, None),
            (RowKind::Context, line(1, "a"), line(1, "a")),
            (RowKind::Added, None, line(2, "b")),
            (RowKind::Hunk, None, None),
            (RowKind::Removed, line(10, "x"), None),
            (RowKind::Context, line(11, "y"), line(11, "y")),
        ]
    );
}

#[test]
fn text_outside_hunks_is_a_note_and_headers_are_dropped() {
    let rows = side_by_side("move from a.bin\nmove to b.bin\nBinary files differ\n");
    let notes: Vec<_> = rows
        .iter()
        .map(|row| (row.kind, row.text.as_str()))
        .collect();
    assert_eq!(
        notes,
        [
            (RowKind::Note, "move from a.bin"),
            (RowKind::Note, "move to b.bin"),
            (RowKind::Note, "Binary files differ"),
        ]
    );
    assert!(side_by_side("--- a\n+++ b\n").is_empty());
    // A blank context line keeps its leading space, so it is a line of both files.
    assert_eq!(
        layout("@@ -1 +1 @@\n \n"),
        [
            (RowKind::Hunk, None, None),
            (RowKind::Context, line(1, ""), line(1, "")),
        ]
    );
}

fn file_diff(patch: &str) -> LoreEvent {
    LoreEvent::FileDiff(LoreFileDiffEventData {
        path: "a.txt".into(),
        patch: patch.into(),
        action: LoreFileAction::Keep,
    })
}

#[test]
fn a_patch_past_the_cap_is_cut_at_a_character_and_marked() {
    // Two-byte characters, so the cap falls inside one unless the cut moves back to a boundary.
    let body: String = std::iter::repeat_n("+é\n", DIFF_CAP / 4 + 10).collect();
    let patch = format!("@@ -00,0 +1,{} @@\n{body}", DIFF_CAP / 4 + 10);
    assert!(patch.len() > DIFF_CAP);
    assert!(!patch.is_char_boundary(DIFF_CAP));

    let Some(DiffMessage::Patch(kept, true)) = DiffMessage::from_event(&file_diff(&patch)) else {
        panic!("a cut patch expected");
    };
    assert!(kept.len() <= DIFF_CAP);
    assert!(patch.starts_with(&kept));

    let mut model = DiffModel::start("a.txt".to_owned(), String::new());
    model.apply(DiffMessage::Patch(kept, true));
    model.apply(DiffMessage::Complete(Ok(())));
    assert!(model.cut);
    assert_eq!(model.progress, Progress::Done);
}

#[test]
fn a_patch_within_the_cap_is_kept_whole() {
    let patch = "@@ -1 +1 @@\n-a\n+b\n";
    assert_eq!(
        DiffMessage::from_event(&file_diff(patch)),
        Some(DiffMessage::Patch(patch.to_owned(), false))
    );
}
