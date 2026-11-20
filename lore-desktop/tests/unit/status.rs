// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore::interface::LoreFileAction;
use lore_desktop::status::FileRow;
use lore_desktop::status::Progress;
use lore_desktop::status::RemoteState;
use lore_desktop::status::Section;
use lore_desktop::status::StatusMessage;
use lore_desktop::status::StatusModel;

fn file(path: &str, section: Section) -> StatusMessage {
    StatusMessage::File(FileRow {
        path: path.to_owned(),
        from_path: None,
        action: LoreFileAction::Keep,
        size: 0,
        section,
    })
}

fn paths(model: &StatusModel, section: Section) -> Vec<&str> {
    model
        .files(section)
        .iter()
        .map(|f| f.path.as_str())
        .collect()
}

#[test]
fn section_follows_staged_and_unresolved_flags() {
    assert_eq!(Section::classify(true, false), Section::Staged);
    assert_eq!(Section::classify(true, true), Section::Unmerged);
    assert_eq!(Section::classify(false, false), Section::Unstaged);
    // An unstaged file is listed as not staged whatever its conflict flag says.
    assert_eq!(Section::classify(false, true), Section::Unstaged);
}

#[test]
fn ahead_and_behind_take_precedence_over_reachability() {
    assert_eq!(
        RemoteState::classify(true, true, true, true, true),
        RemoteState::Diverged
    );
    assert_eq!(
        RemoteState::classify(true, true, true, true, false),
        RemoteState::Ahead
    );
    assert_eq!(
        RemoteState::classify(true, true, true, false, true),
        RemoteState::Behind
    );
    // Offline, a branch with local revisions still reads as ahead, as `lore status` says.
    assert_eq!(
        RemoteState::classify(false, false, false, true, false),
        RemoteState::Ahead
    );
}

#[test]
fn remote_state_without_divergence() {
    assert_eq!(
        RemoteState::classify(false, false, false, false, false),
        RemoteState::Unavailable
    );
    assert_eq!(
        RemoteState::classify(true, false, false, false, false),
        RemoteState::Unauthorized
    );
    assert_eq!(
        RemoteState::classify(true, true, false, false, false),
        RemoteState::NoBranch
    );
    assert_eq!(
        RemoteState::classify(true, true, true, false, false),
        RemoteState::InSync
    );
}

#[test]
fn files_go_to_their_section_and_sort_on_completion() {
    let mut model = StatusModel::default();
    model.start();
    model.apply(StatusMessage::Begin);
    for message in [
        file("b", Section::Staged),
        file("c", Section::Unstaged),
        file("a", Section::Staged),
        file("m", Section::Unmerged),
    ] {
        model.apply(message);
    }
    assert!(model.is_running());
    model.apply(StatusMessage::Complete(Ok(())));

    assert_eq!(model.progress, Progress::Done);
    assert_eq!(paths(&model, Section::Staged), ["a", "b"]);
    assert_eq!(paths(&model, Section::Unmerged), ["m"]);
    assert_eq!(paths(&model, Section::Unstaged), ["c"]);
}

/// A model holding `files` from a finished call, each with its section.
fn finished(files: &[(&str, Section)]) -> StatusModel {
    let mut model = StatusModel::default();
    model.start();
    model.apply(StatusMessage::Begin);
    for (path, section) in files {
        model.apply(file(path, *section));
    }
    model.apply(StatusMessage::Complete(Ok(())));
    model
}

#[test]
fn a_change_that_fails_keeps_the_last_status_and_selection() {
    let mut model = finished(&[("a", Section::Unstaged)]);
    model.toggle(Section::Unstaged, 0);

    model.start();
    assert_eq!(model.progress, Progress::Running);
    model.apply(StatusMessage::Complete(Err("no instance".to_owned())));

    assert!(matches!(model.progress, Progress::Failed(_)));
    assert_eq!(paths(&model, Section::Unstaged), ["a"]);
    assert_eq!(model.selected_paths(Section::Unstaged), ["a"]);
}

#[test]
fn progress_shows_while_running_and_clears_when_the_call_ends() {
    for outcome in [Ok(()), Err("offline".to_owned())] {
        let mut model = StatusModel::default();
        model.start();
        model.apply(StatusMessage::Progress(
            "Syncing 1/4 files".to_owned(),
            Some(0.25),
        ));
        assert_eq!(
            model.note,
            Some(("Syncing 1/4 files".to_owned(), Some(0.25)))
        );
        model.apply(StatusMessage::Complete(outcome));
        assert!(model.note.is_none());
    }
}

#[test]
fn the_user_outlives_the_status_read_it_preceded() {
    let mut model = StatusModel::default();
    model.start();
    model.apply(StatusMessage::User(Some("ada".to_owned())));
    model.apply(StatusMessage::Begin);
    model.apply(StatusMessage::Complete(Ok(())));
    assert_eq!(model.user.as_deref(), Some("ada"));

    model.start();
    model.apply(StatusMessage::User(None));
    assert!(model.user.is_none());
}

#[test]
fn begin_replaces_the_last_status_and_its_selection() {
    let mut model = finished(&[("a", Section::Unstaged)]);
    model.toggle(Section::Unstaged, 0);

    model.start();
    model.apply(StatusMessage::Begin);

    assert!(model.files(Section::Unstaged).is_empty());
    assert!(model.selected_paths(Section::Unstaged).is_empty());
    assert!(model.revision.is_none());
}

#[test]
fn selection_toggles_per_section_in_display_order() {
    let mut model = finished(&[
        ("c", Section::Unstaged),
        ("a", Section::Unstaged),
        ("b", Section::Unstaged),
        ("s", Section::Staged),
    ]);
    // Rows sort on completion, so index 2 is "c" and index 0 is "a".
    model.toggle(Section::Unstaged, 2);
    model.toggle(Section::Unstaged, 0);
    assert_eq!(model.selected_paths(Section::Unstaged), ["a", "c"]);
    assert!(model.selected_paths(Section::Staged).is_empty());

    model.toggle(Section::Unstaged, 2);
    assert_eq!(model.selected_paths(Section::Unstaged), ["a"]);
    assert!(model.is_selected(Section::Unstaged, 0));
    assert!(!model.is_selected(Section::Unstaged, 2));
}
