use alloc::boxed::Box;
use crate::app::{Command, NodeResult};
use crate::app::ui_node::{UiNode, render_menu_screen, render_brightness_popup};
use crate::input::EncoderEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

// =============================================================================
// 设置菜单
// =============================================================================

#[derive(Debug)]
pub struct SettingsMenu {
    selected: usize,
}

impl SettingsMenu {
    const ITEMS: &[&str] = &["Brightness"];

    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl UiNode for SettingsMenu {
    fn label(&self) -> &'static str {
        "Settings"
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
                    0 => (NodeResult::Push(Box::new(BrightnessNode::new())), alloc::vec::Vec::new()),
                    _ => (NodeResult::Stay, alloc::vec::Vec::new()),
                };
            }
            EncoderEvent::BackPressed => {
                return (NodeResult::Pop, alloc::vec::Vec::new());
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        render_menu_screen(
            frame,
            "Settings",
            Self::ITEMS,
            self.selected,
            "Rotate: Navigate | Confirm: Select | Back: Exit",
        );
    }
}

// =============================================================================
// 亮度调节节点
// =============================================================================

#[derive(Debug)]
pub struct BrightnessNode {
    brightness: u8,
}

impl BrightnessNode {
    pub fn new() -> Self {
        Self { brightness: 100 }
    }
}

impl UiNode for BrightnessNode {
    fn label(&self) -> &'static str {
        "Brightness"
    }

    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>) {
        match event {
            EncoderEvent::Clockwise => {
                if self.brightness < 100 {
                    self.brightness += 1;
                }
                return (
                    NodeResult::Stay,
                    alloc::vec![Command::Backlight(self.brightness)],
                );
            }
            EncoderEvent::CounterClockwise => {
                if self.brightness > 1 {
                    self.brightness -= 1;
                }
                return (
                    NodeResult::Stay,
                    alloc::vec![Command::Backlight(self.brightness)],
                );
            }
            EncoderEvent::ConfirmReleased | EncoderEvent::BackPressed => {
                return (NodeResult::Pop, alloc::vec::Vec::new());
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        render_brightness_popup(frame, self.brightness);
    }
}
