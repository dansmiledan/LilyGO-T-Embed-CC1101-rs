use crate::input::EncoderEvent;
use ratatui::prelude::*;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Gauge, List, ListItem, Paragraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    BrightnessSettings,
}

pub struct App {
    state: AppState,
    selected: usize,
    menu_items: &'static [&'static str],
    brightness: u8,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Menu,
            selected: 0,
            menu_items: &["Wi-Fi", "Bluetooth", "RFID/NFC", "Sub-GHz", "IR Remote", "GPS", "Settings"],
            brightness: 16,
        }
    }

    pub fn handle_event(&mut self, event: EncoderEvent) {
        match self.state {
            AppState::Menu => {
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
                    EncoderEvent::ConfirmReleased => {
                        // 如果选中 Settings，进入亮度调节界面
                        if self.selected == 6 {
                            self.state = AppState::BrightnessSettings;
                        }
                    }
                    _ => {}
                }
            }
            AppState::BrightnessSettings => {
                match event {
                    EncoderEvent::Clockwise => {
                        if self.brightness < 16 {
                            self.brightness = self.brightness + 1;
                        }
                    }
                    EncoderEvent::CounterClockwise => {
                        if self.brightness > 1 {
                            self.brightness = self.brightness - 1;
                        }
                    }
                    EncoderEvent::BackPressed => {
                        // 返回菜单，重置选择项到 Settings
                        self.state = AppState::Menu;
                        self.selected = 6;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        match self.state {
            AppState::Menu => self.render_menu(frame),
            AppState::BrightnessSettings => self.render_brightness_settings(frame),
        }
    }

    fn render_menu(&self, frame: &mut Frame) {
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
        let hint = Paragraph::new("Rotate: Navigate | Confirm: Select | Back: Exit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }

    fn render_brightness_settings(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.area());

        // 标题栏
        let title = Paragraph::new("Brightness Settings")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .alignment(Alignment::Center);

        frame.render_widget(title, chunks[0]);

        // 亮度百分比显示
        let percentage = (self.brightness as u32 * 100) / 16;
        let percentage_text = alloc::format!("{}%", percentage);
        let brightness_display = Paragraph::new(percentage_text)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center);

        frame.render_widget(brightness_display, chunks[1]);

        // 进度条
        let gauge = Gauge::default()
            .ratio(self.brightness as f64 / 16.0)
            .label(alloc::format!("{}/16", self.brightness))
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(ratatui::widgets::Borders::ALL));

        frame.render_widget(gauge, chunks[2]);

        // 帮助提示
        let hint = Paragraph::new("Rotate: Adjust | Back: Exit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[3]);
    }

    pub fn get_brightness(&self) -> u8 {
        self.brightness
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
