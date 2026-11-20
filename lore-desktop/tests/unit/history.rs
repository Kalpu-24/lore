// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore::interface::LoreFileAction;
use lore_desktop::history::ChangeRow;
use lore_desktop::history::HistoryMessage;
use lore_desktop::history::HistoryModel;
use lore_desktop::history::RevisionRow;
use lore_desktop::status::Progress;

fn revision(number: u64) -> HistoryMessage {
    HistoryMessage::Revision(RevisionRow {
        number,
        revision: format!("r{number}"),
        ..Default::default()
    })
}

fn change(path: &str) -> HistoryMessage {
    HistoryMessage::Change(ChangeRow {
        path: path.to_owned(),
        from_path: None,
        action: LoreFileAction::Add,
        size: 1,
    })
}

#[test]
fn metadata_belongs_to_the_revision_reported_before_it() {
    let mut model = HistoryModel::default();
    model.start();
    // Metadata with no revision before it has no row to go to.
    model.apply(HistoryMessage::Message("stray".to_owned()));
    model.apply(revision(2));
    model.apply(HistoryMessage::Message("Add maps\n\nLong body".to_owned()));
    model.apply(HistoryMessage::Timestamp(1_700_000_000_000));
    model.apply(revision(1));
    model.apply(HistoryMessage::CreatedBy("ada".to_owned()));
    model.apply(HistoryMessage::Complete(Ok(())));

    assert_eq!(model.progress, Progress::Done);
    let [newer, older] = model.revisions.as_slice() else {
        panic!("two revisions expected: {:?}", model.revisions);
    };
    assert_eq!(newer.number, 2);
    assert_eq!(newer.summary(), "Add maps");
    assert_eq!(newer.timestamp, Some(1_700_000_000_000));
    assert!(newer.created_by.is_empty());
    assert_eq!(older.created_by, "ada");
    assert!(older.message.is_empty());
    assert_eq!(older.summary(), "");
}

#[test]
fn changes_of_the_selected_revision_sort_and_reset_on_selection() {
    let mut model = HistoryModel::default();
    model.start();
    model.apply(revision(1));
    model.apply(revision(2));
    model.apply(HistoryMessage::Complete(Ok(())));

    model.select(1);
    assert_eq!(model.selected_revision().map(|r| r.number), Some(2));
    assert_eq!(model.changes_progress, Progress::Running);
    model.apply_changes(change("b"));
    model.apply_changes(change("a"));
    // A revision listed in the info call adds no change.
    model.apply_changes(revision(9));
    model.apply_changes(HistoryMessage::Complete(Ok(())));
    let paths: Vec<_> = model.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["a", "b"]);

    model.select(0);
    assert!(model.changes.is_empty());
    model.apply_changes(HistoryMessage::Complete(Err("gone".to_owned())));
    assert!(matches!(model.changes_progress, Progress::Failed(_)));
}

#[test]
fn a_new_history_clears_the_last_one_and_its_selection() {
    let mut model = HistoryModel::default();
    model.start();
    model.apply(revision(1));
    model.apply(HistoryMessage::Complete(Ok(())));
    model.select(0);
    model.apply_changes(change("a"));

    model.start();
    assert!(model.revisions.is_empty());
    assert!(model.selected_revision().is_none());
    assert!(model.changes.is_empty());
    assert_eq!(model.changes_progress, Progress::Idle);
    assert_eq!(model.progress, Progress::Running);
}
