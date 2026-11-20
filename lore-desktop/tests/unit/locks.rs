// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore_desktop::locks::LockMessage;
use lore_desktop::locks::LockModel;
use lore_desktop::locks::LockRow;
use lore_desktop::status::Progress;

fn lock(path: &str) -> LockMessage {
    LockMessage::Lock(LockRow {
        path: path.to_owned(),
        owner: "ada".to_owned(),
        locked_at: 0,
    })
}

fn finished(paths: &[&str]) -> LockModel {
    let mut model = LockModel::default();
    model.start();
    for path in paths {
        model.apply(lock(path));
    }
    model.apply(LockMessage::Complete(Ok(())));
    model
}

#[test]
fn locks_sort_by_path_and_selection_follows_display_order() {
    let mut model = finished(&["c.umap", "a.uasset", "b.uasset"]);
    assert_eq!(model.progress, Progress::Done);
    let paths: Vec<_> = model.rows.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(paths, ["a.uasset", "b.uasset", "c.umap"]);

    model.toggle(2);
    model.toggle(0);
    assert_eq!(model.selected_paths(), ["a.uasset", "c.umap"]);
    model.toggle(2);
    assert_eq!(model.selected_paths(), ["a.uasset"]);
    assert!(model.is_selected(0));
}

#[test]
fn a_new_query_clears_the_locks_and_the_selection() {
    let mut model = finished(&["a.uasset"]);
    model.toggle(0);

    model.start();
    assert!(model.rows.is_empty());
    assert!(model.selected_paths().is_empty());
    model.apply(LockMessage::Complete(Err("offline".to_owned())));
    assert!(matches!(model.progress, Progress::Failed(_)));
}
