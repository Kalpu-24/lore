// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
use lore_desktop::notify::Live;
use lore_desktop::notify::LiveModel;
use lore_desktop::notify::NotifyMessage;

#[test]
fn a_notification_marks_the_views_stale_once() {
    let mut model = LiveModel::default();
    model.start("/work/game");
    assert_eq!(model.live, Live::Subscribing);
    model.apply(NotifyMessage::Subscribed);
    assert_eq!(model.live, Live::On);

    assert!(!model.take_stale());
    model.apply(NotifyMessage::Changed);
    model.apply(NotifyMessage::Changed);
    // Two changes before the views are read again need one read.
    assert!(model.take_stale());
    assert!(!model.take_stale());
}

#[test]
fn focus_refreshes_only_without_notifications() {
    let mut model = LiveModel::default();
    model.start("/work/game");
    model.apply(NotifyMessage::Subscribed);
    model.focused();
    assert!(!model.take_stale());

    model.apply(NotifyMessage::Failed("offline".to_owned()));
    assert!(matches!(model.live, Live::Unavailable(_)));
    model.focused();
    assert!(model.take_stale());
}
