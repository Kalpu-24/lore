// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore_desktop::branches::BranchMessage;
use lore_desktop::branches::BranchModel;
use lore_desktop::branches::BranchRow;
use lore_desktop::status::Progress;

fn branch(name: &str, remote: bool, current: bool) -> BranchMessage {
    BranchMessage::Branch(BranchRow {
        name: name.to_owned(),
        remote,
        current,
        latest: String::new(),
    })
}

#[test]
fn branches_sort_by_name_with_the_local_copy_first() {
    let mut model = BranchModel::default();
    model.start();
    for message in [
        branch("main", true, false),
        branch("feature", false, false),
        branch("main", false, true),
    ] {
        model.apply(message);
    }
    model.apply(BranchMessage::Complete(Ok(())));

    let rows: Vec<_> = model
        .rows
        .iter()
        .map(|row| (row.name.as_str(), row.remote, row.current))
        .collect();
    assert_eq!(
        rows,
        [
            ("feature", false, false),
            ("main", false, true),
            ("main", true, false),
        ]
    );
    assert_eq!(model.progress, Progress::Done);
}

#[test]
fn a_new_list_clears_the_selection() {
    let mut model = BranchModel::default();
    model.start();
    model.apply(branch("main", false, true));
    model.apply(BranchMessage::Complete(Ok(())));
    model.selected = Some(0);
    assert_eq!(
        model.selected_row().map(|row| row.name.as_str()),
        Some("main")
    );

    model.start();
    assert!(model.selected_row().is_none());
    model.apply(BranchMessage::Complete(Err("offline".to_owned())));
    assert!(matches!(model.progress, Progress::Failed(_)));
}
