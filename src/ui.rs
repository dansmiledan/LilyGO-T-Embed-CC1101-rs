use crate::input::EncoderEvent;
use ratatui::prelude::*;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};

pub struct App {
    selected: usize,
    menu_items: &'static [&'static str],
}

impl App {
    pub fn new() -> Self {
        Self {
            selected: 0,
            menu_items: &["Wi-Fi", "Bluetooth", "RFID/NFC", "Sub-GHz", "IR Remote", "GPS", "Settings"],
        }
    }

    pub fn handle_event(&mut self, event: EncoderEvent) {
        match event {
            EncoderEvent::Clockwise => {
                if self.selected < self.menu_items.len() - 1 {
                    self.selected += 1;
                }
            }
            EncoderEvent::CounterClockwise => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            _ => {}
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(frame.area());

        // 标题栏
        let title = Paragraph::new("T-Embed CC1101")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .alignment(Alignment::Center);

        frame.render_widget(title, chunks[0]);

        // 菜单列表
        let items: alloc::vec::Vec<ListItem> = self
            .menu_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::White).bold()
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(*item).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title("Menu"),
            )
            .style(Style::default().bg(Color::Black))
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("> ");

        frame.render_widget(list, chunks[1]);

        // 帮助提示
        let hint = Paragraph::new("Rotate: Navigate | Press: Select | LongPress: Back")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
