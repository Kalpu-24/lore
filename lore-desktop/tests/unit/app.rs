// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use std::time::Duration;
use std::time::Instant;

use eframe::egui;
use lore::interface::LoreFileAction;
use lore_desktop::app::LoreDesktop;
use lore_desktop::status::FileRow;
use lore_desktop::status::Section;
use lore_desktop::status::StatusMessage;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

/// A window whose one tab's status holds `rows` unstaged files.
fn window_with(rows: usize) -> LoreDesktop {
    let mut app = LoreDesktop::new(paths(&["instance"]), None);
    let status = &mut app.tabs[0].status;
    status.start();
    status.apply(StatusMessage::Begin);
    for i in 0..rows {
        status.apply(StatusMessage::File(FileRow {
            path: format!("Content/Maps/Level{i:07}.umap"),
            from_path: None,
            action: LoreFileAction::Keep,
            size: 1 << 20,
            section: Section::Unstaged,
        }));
    }
    status.apply(StatusMessage::Complete(Ok(())));
    app
}

fn titles(app: &LoreDesktop) -> Vec<String> {
    app.tabs.iter().map(|tab| tab.title()).collect()
}

#[test]
fn remembered_instances_open_as_tabs_with_the_opened_one_active() {
    let app = LoreDesktop::new(paths(&["/w/a", "", "/w/b"]), Some("/w/b".to_owned()));
    assert_eq!(titles(&app), ["a", "b"]);
    assert_eq!(app.active, 1);

    let app = LoreDesktop::new(paths(&["/w/a"]), Some("/w/c/".to_owned()));
    assert_eq!(titles(&app), ["a", "c"]);
    assert_eq!(app.active, 1);
    assert_eq!(app.open_paths(), ["/w/a", "/w/c/"]);
}

#[test]
fn no_instances_open_one_empty_tab_and_none_is_remembered() {
    let app = LoreDesktop::new(Vec::new(), None);
    assert_eq!(titles(&app), ["New instance"]);
    assert!(app.open_paths().is_empty());
}

#[test]
fn closing_a_tab_keeps_the_active_one_in_view() {
    let mut app = LoreDesktop::new(paths(&["a", "b", "c"]), Some("c".to_owned()));
    // Closing a tab before the active one keeps the same tab active.
    app.close(0);
    assert_eq!(titles(&app), ["b", "c"]);
    assert_eq!(app.active, 1);
    // Closing the active last tab moves to the one before it.
    app.close(1);
    assert_eq!(titles(&app), ["b"]);
    assert_eq!(app.active, 0);
    // Closing the only tab leaves an empty one.
    app.close(0);
    assert_eq!(titles(&app), ["New instance"]);
    assert_eq!(app.active, 0);
}

/// Draws `frames` frames of a 960 × 640 window, and returns the shapes the last one drew and the
/// mean time of a frame.
fn draw(app: &mut LoreDesktop, frames: u32) -> (usize, Duration) {
    let ctx = egui::Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(960.0, 640.0),
        )),
        ..Default::default()
    };
    let mut frame = || {
        let mut output = ctx.run_ui(input(), |ui| app.show(ui));
        // No renderer takes the font atlas here.
        output.textures_delta.clear();
        output.shapes.len()
    };
    // The first frames lay out fonts and sizes; time only the frames after.
    frame();
    frame();
    let start = Instant::now();
    let mut shapes = 0;
    for _ in 0..frames {
        shapes = frame();
    }
    (shapes, start.elapsed() / frames)
}

#[test]
fn status_draws_only_the_rows_in_view() {
    let (small_shapes, small_time) = draw(&mut window_with(100), 20);
    let (large_shapes, large_time) = draw(&mut window_with(1_000_000), 20);
    eprintln!(
        "100 rows: {small_shapes} shapes, {small_time:?}/frame; \
         1,000,000 rows: {large_shapes} shapes, {large_time:?}/frame"
    );
    // Both fill the window, so a frame draws the same rows whatever the status holds.
    assert!(
        large_shapes <= small_shapes + 16,
        "{large_shapes} shapes for 1M rows against {small_shapes} for 100"
    );
}
