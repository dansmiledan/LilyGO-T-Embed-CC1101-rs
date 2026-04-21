use alloc::boxed::Box;
use crate::app::{Command, NodeResult};
use crate::app::ui_node::{UiNode, render_menu_screen};
use crate::ble_hid::{BleControlEvent, BleKeyEvent};
use crate::input::EncoderEvent;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

// =============================================================================
// BLE 菜单
// =============================================================================

#[derive(Debug)]
pub struct BleMenu {
    selected: usize,
}

impl BleMenu {
    const ITEMS: &[&str] = &["BLE Keyboard", "BLE Spam"];

    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl UiNode for BleMenu {
    fn label(&self) -> &'static str {
        "BLE"
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
                    0 => (NodeResult::Push(Box::new(BleKeyboardNode::new())), alloc::vec::Vec::new()),
                    1 => (NodeResult::Push(Box::new(BleSpamNode::new())), alloc::vec::Vec::new()),
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
            "BLE",
            Self::ITEMS,
            self.selected,
            "Rotate: Navigate | Confirm: Select | Back: Exit",
        );
    }
}

// =============================================================================
// BLE 键盘节点
// =============================================================================

#[derive(Debug)]
pub struct BleKeyboardNode;

impl BleKeyboardNode {
    pub fn new() -> Self {
        Self
    }
}

impl UiNode for BleKeyboardNode {
    fn label(&self) -> &'static str {
        "BLE Keyboard"
    }

    fn on_enter(&mut self) -> alloc::vec::Vec<Command> {
        alloc::vec![Command::BleControl(BleControlEvent::Start)]
    }

    fn on_exit(&mut self) -> alloc::vec::Vec<Command> {
        alloc::vec![Command::BleControl(BleControlEvent::Stop)]
    }

    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>) {
        match event {
            EncoderEvent::Clockwise => {
                return (NodeResult::Stay, alloc::vec![Command::BleKey(BleKeyEvent::Down)]);
            }
            EncoderEvent::CounterClockwise => {
                return (NodeResult::Stay, alloc::vec![Command::BleKey(BleKeyEvent::Up)]);
            }
            EncoderEvent::BackPressed => {
                return (NodeResult::Pop, alloc::vec::Vec::new());
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        let area = frame.area();
        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        let title = Paragraph::new("BLE Keyboard Mode")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Blue))
            .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let content = Paragraph::new(
            "Rotary encoder sends arrow keys:\n\n\
             ↓ Clockwise: Down arrow\n\
             ↑ Counter-Clockwise: Up arrow\n\n\
             Back button to exit"
        )
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title("Status"),
        )
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .alignment(Alignment::Center);
        frame.render_widget(content, chunks[1]);

        let hint = Paragraph::new("Rotate: Send keys | Back: Exit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }
}

// =============================================================================
// BLE Spam 节点（placeholder）
// =============================================================================

#[derive(Debug)]
pub struct BleSpamNode;

impl BleSpamNode {
    pub fn new() -> Self {
        Self
    }
}

impl UiNode for BleSpamNode {
    fn label(&self) -> &'static str {
        "BLE Spam"
    }

    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>) {
        match event {
            EncoderEvent::BackPressed => {
                return (NodeResult::Pop, alloc::vec::Vec::new());
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        let area = frame.area();
        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        let title = Paragraph::new("BLE Spam")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let content = Paragraph::new("Broadcasting BLE advertisement spam...\n\n(Not yet implemented)")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title("Status"),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .alignment(Alignment::Center);
        frame.render_widget(content, chunks[1]);

        let hint = Paragraph::new("Back: Exit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }
}
