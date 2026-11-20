// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use std::path::Path;

use lore::remote::command::LoreCommand;
use lore_desktop::worker::Change;
use lore_desktop::worker::change_command;

fn stage_paths(command: Option<LoreCommand>) -> Vec<String> {
    let Some(LoreCommand::FileStage(args)) = command else {
        panic!("not a stage command: {command:?}");
    };
    args.paths
        .as_slice()
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect()
}

#[test]
fn change_paths_are_made_absolute_under_the_instance() {
    let paths = stage_paths(change_command(
        "/work/game",
        Change::Stage(vec!["Content/a.uasset".to_owned(), String::new()]),
    ));
    let expected = Path::new("/work/game").join("Content/a.uasset");
    // The root is the instance itself, which stages every dirty file.
    assert_eq!(paths, [expected.to_string_lossy().as_ref(), "/work/game"]);
}

#[test]
fn status_alone_runs_no_change() {
    assert!(change_command("/work/game", Change::None).is_none());
}
