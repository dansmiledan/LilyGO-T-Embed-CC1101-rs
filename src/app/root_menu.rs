use crate::app::{Command, NodeResult};
use crate::app::ui_node::{UiNode, render_menu_screen};
use crate::app::ble::BleMenu;
use crate::app::settings::SettingsMenu;
use crate::app::sd_browser::SdBrowserNode;
use crate::input::EncoderEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

#[derive(Debug)]
pub struct RootMenu {
    selected: usize,
}

impl RootMenu {
    const ITEMS: &[&str] = &["BLE", "Settings", "SD Card"];

    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl UiNode for RootMenu {
    fn label(&self) -> &'static str {
        "Main Menu"
    }

    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>) {
        match event {
            EncoderEvent::Clockwise => {
                if self.selected < Self::ITEMS.len() - 1 {
                    self.selected += 1;
                }
            }
            EncoderEvent::CounterClockwise => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            EncoderEvent::ConfirmReleased => {
                return match self.selected {
                    0 => (NodeResult::Push(alloc::boxed::Box::new(BleMenu::new())), alloc::vec::Vec::new()),
                    1 => (NodeResult::Push(alloc::boxed::Box::new(SettingsMenu::new())), alloc::vec::Vec::new()),
                    2 => (NodeResult::Push(alloc::boxed::Box::new(SdBrowserNode::new())), alloc::vec::Vec::new()),
                    _ => (NodeResult::Stay, alloc::vec::Vec::new()),
                };
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        render_menu_screen(
            frame,
            "T-Embed CC1101",
            Self::ITEMS,
            self.selected,
            "Rotate: Navigate | Confirm: Select",
        );
    }
}
