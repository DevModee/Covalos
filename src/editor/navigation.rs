//! Back/forward history across the main views and the Escape key for overlays.

use std::path::PathBuf;

use eframe::egui::{self, Key, Modifiers, PointerButton};

use super::Editor;

const LIMIT: usize = 64;

/// What the user was looking at: the main view, the active file and the docked chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Place {
    hub: bool,
    studio: bool,
    file: Option<PathBuf>,
    chat: Option<String>,
}

#[derive(Default)]
pub(super) struct Navigation {
    back: Vec<Place>,
    forward: Vec<Place>,
    current: Option<Place>,
    /// Set at the start of a frame so overlays that close on Escape by themselves
    /// (popups, focused text fields, the companion) are not closed twice.
    escape_taken: bool,
}

impl Navigation {
    pub(super) fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub(super) fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    fn record(&mut self, place: Place) {
        if self.current.as_ref() == Some(&place) {
            return;
        }
        if let Some(previous) = self.current.replace(place) {
            self.back.push(previous);
            if self.back.len() > LIMIT {
                self.back.remove(0);
            }
            self.forward.clear();
        }
    }
}

impl Editor {
    fn place(&self) -> Place {
        Place {
            hub: self.hub_open,
            studio: self.studio_open,
            file: self
                .active
                .and_then(|index| self.documents.get(index))
                .map(|doc| doc.path.clone()),
            chat: self.hub.agents.chat.dock.clone(),
        }
    }

    fn go_to(&mut self, place: Place) {
        self.hub_open = place.hub;
        self.studio_open = place.studio;
        if let Some(path) = &place.file
            && let Some(index) = self.documents.iter().position(|doc| &doc.path == path)
        {
            self.activate(index);
        }
        if place.chat.is_some() {
            self.hub.agents.chat.dock.clone_from(&place.chat);
            self.remember_chat_layout();
        }
        // Store what is actually shown, in case the file was closed meanwhile.
        self.nav.current = Some(self.place());
    }

    pub(super) fn go_back(&mut self) {
        if let Some(target) = self.nav.back.pop() {
            let here = self.place();
            self.nav.forward.push(here);
            self.go_to(target);
        }
    }

    pub(super) fn go_forward(&mut self) {
        if let Some(target) = self.nav.forward.pop() {
            let here = self.place();
            self.nav.back.push(here);
            self.go_to(target);
        }
    }

    /// Runs before any panel is drawn.
    pub(super) fn begin_navigation(&mut self, ctx: &egui::Context) {
        self.nav.escape_taken =
            egui::Popup::is_any_open(ctx) || ctx.wants_keyboard_input() || self.companion.open;
        let typing = ctx.wants_keyboard_input();
        let (back, forward) = ctx.input_mut(|i| {
            let mouse_back = i.pointer.button_pressed(PointerButton::Extra1);
            let mouse_forward = i.pointer.button_pressed(PointerButton::Extra2);
            let key_back = !typing && i.consume_key(Modifiers::ALT, Key::ArrowLeft);
            let key_forward = !typing && i.consume_key(Modifiers::ALT, Key::ArrowRight);
            (mouse_back || key_back, mouse_forward || key_forward)
        });
        if back {
            self.go_back();
        } else if forward {
            self.go_forward();
        }
    }

    /// Runs after every panel and modal had its chance to react to the frame's input.
    pub(super) fn end_navigation(&mut self, ctx: &egui::Context) {
        let blocked = self.nav.escape_taken
            || self.pending.is_some()
            || self.pending_delete.is_some()
            || self.palette.is_some()
            || self.settings_open
            || self.notifications_open
            || self.conversation_exit_warning;
        if !blocked && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.close_top_overlay();
        }
        let place = self.place();
        self.nav.record(place);
    }

    /// Closes the surface drawn on top of everything else, one per Escape press.
    fn close_top_overlay(&mut self) {
        if self.about_open {
            self.about_open = false;
        } else if self.guided.open {
            self.guided.open = false;
        } else if !self.hub.agents.chat.detached.is_empty() {
            self.hub.agents.chat.detached.pop();
            self.remember_chat_layout();
        } else if self.studio_open {
            self.studio_open = false;
        } else if self.hub_open {
            self.hub_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(file: &str) -> Place {
        Place {
            hub: false,
            studio: false,
            file: Some(PathBuf::from(file)),
            chat: None,
        }
    }

    #[test]
    fn history_records_changes_and_clears_forward_on_new_place() {
        let mut nav = Navigation::default();
        nav.record(at("a"));
        nav.record(at("a"));
        nav.record(at("b"));
        assert_eq!(nav.back, vec![at("a")]);
        nav.forward.push(at("c"));
        nav.record(at("d"));
        assert!(nav.forward.is_empty());
        assert_eq!(nav.back, vec![at("a"), at("b")]);
    }
}
