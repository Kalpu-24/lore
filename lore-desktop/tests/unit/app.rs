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

/// A window whose status holds `rows` unstaged files.
fn window_with(rows: usize) -> LoreDesktop {
    let mut app = LoreDesktop::new(Some("instance".to_owned()));
    app.status.start();
    app.status.apply(StatusMessage::Begin);
    for i in 0..rows {
        app.status.apply(StatusMessage::File(FileRow {
            path: format!("Content/Maps/Level{i:07}.umap"),
            from_path: None,
            action: LoreFileAction::Keep,
            size: 1 << 20,
            section: Section::Unstaged,
        }));
    }
    app.status.apply(StatusMessage::Complete(Ok(())));
    app
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
