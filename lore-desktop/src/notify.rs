// SPDX-FileCopyrightText: 2026 Epic Games, Inc.
// SPDX-License-Identifier: MIT
//! Live updates: what the remote's notifications mean to a tab.
//!
//! A subscription is a call whose callback the library keeps after the call returns, and passes
//! each notification for the repository to. A notification only marks the tab stale; the tab reads
//! its views again once no call of its own is running.

use lore::interface::LoreEvent;

/// What a notification subscription reports, copied out of its events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyMessage {
    Subscribed,
    /// Something the views show changed on the remote: a branch was pushed, created or deleted,
    /// or a file was locked or unlocked.
    Changed,
    /// The subscription could not be made: `Err` carries the library's error message.
    Failed(String),
}

impl NotifyMessage {
    pub fn from_event(event: &LoreEvent) -> Option<Self> {
        match event {
            LoreEvent::NotificationSubscribed(_) => Some(Self::Subscribed),
            LoreEvent::NotificationBranchPushed(_)
            | LoreEvent::NotificationBranchCreated(_)
            | LoreEvent::NotificationBranchDeleted(_)
            | LoreEvent::NotificationResourceLocked(_)
            | LoreEvent::NotificationResourceUnlocked(_) => Some(Self::Changed),
            LoreEvent::Complete(data) if data.status != 0 => {
                Some(Self::Failed(data.error.message.as_str().to_owned()))
            }
            _ => None,
        }
    }
}

/// Where a tab's subscription stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Live {
    /// No subscription has been asked for.
    #[default]
    Off,
    Subscribing,
    /// Notifications arrive; the views refresh when one does.
    On,
    /// No notifications arrive; the views refresh when the window gains focus.
    Unavailable(String),
}

/// A tab's live-update state: its subscription, and whether its views are out of date.
#[derive(Debug, Default)]
pub struct LiveModel {
    pub live: Live,
    /// The instance the subscription is for.
    pub path: Option<String>,
    /// A change on the remote, or a return of focus without notifications, left the views out of
    /// date.
    pub stale: bool,
}

impl LiveModel {
    /// Notes a subscription asked for on `path`.
    pub fn start(&mut self, path: &str) {
        self.path = Some(path.to_owned());
        self.live = Live::Subscribing;
    }

    pub fn apply(&mut self, message: NotifyMessage) {
        match message {
            NotifyMessage::Subscribed => self.live = Live::On,
            NotifyMessage::Changed => self.stale = true,
            NotifyMessage::Failed(error) => self.live = Live::Unavailable(error),
        }
    }

    /// The window gained focus: without notifications, the views may be out of date.
    pub fn focused(&mut self) {
        if matches!(self.live, Live::Unavailable(_)) {
            self.stale = true;
        }
    }

    /// Whether the views are out of date, clearing the mark: the caller reads them again.
    pub fn take_stale(&mut self) -> bool {
        std::mem::take(&mut self.stale)
    }
}
