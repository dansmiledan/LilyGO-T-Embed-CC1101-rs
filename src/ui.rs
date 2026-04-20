use crate::input::EncoderEvent;
use ratatui::prelude::*;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Clear, Gauge, List, ListItem, Paragraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Wifi,
    Bluetooth,
    BleKeyboard,
    RfidNfc,
    SubGhz,
    IrRemote,
    Settings,
}

impl MenuItem {
    pub const ALL: &'static [MenuItem] = &[
        MenuItem::Wifi,
        MenuItem::Bluetooth,
        MenuItem::BleKeyboard,
        MenuItem::RfidNfc,
        MenuItem::SubGhz,
        MenuItem::IrRemote,
        MenuItem::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Wifi => "Wi-Fi",
            MenuItem::Bluetooth => "Bluetooth",
            MenuItem::BleKeyboard => "BLE Keyboard",
            MenuItem::RfidNfc => "RFID/NFC",
            MenuItem::SubGhz => "Sub-GHz",
            MenuItem::IrRemote => "IR Remote",
            MenuItem::Settings => "Settings",
        }
    }

    pub fn is_settings(self) -> bool {
        matches!(self, MenuItem::Settings)
    }
    
    pub fn is_ble_keyboard(self) -> bool {
        matches!(self, MenuItem::BleKeyboard)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Menu,
    BrightnessPopup,
    BleKeyboardMode,
}

pub struct App {
    state: AppState,
    selected: usize,
    menu_items: &'static [MenuItem],
    brightness: u8,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Menu,
            selected: 0,
            menu_items: MenuItem::ALL,
            brightness: 100,
        }
    }
    
    /// 检查是否处于 BLE 键盘模式
    pub fn is_ble_mode(&self) -> bool {
        matches!(self.state, AppState::BleKeyboardMode)
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
                        // 如果选中 Settings，弹出亮度调节窗口
                        // 如果选中 BLE Keyboard，进入 BLE 键盘模式
                        if let Some(item) = self.menu_items.get(self.selected) {
                            if item.is_settings() {
                                self.state = AppState::BrightnessPopup;
                            } else if item.is_ble_keyboard() {
                                self.state = AppState::BleKeyboardMode;
                            }
                        }
                    }
                    _ => {}
                }
            }
            AppState::BrightnessPopup => {
                match event {
                    EncoderEvent::Clockwise => {
                        if self.brightness < 100 {
                            self.brightness = self.brightness + 1;
                        }
                    }
                    EncoderEvent::CounterClockwise => {
                        if self.brightness > 1 {
                            self.brightness = self.brightness - 1;
                        }
                    }
                    EncoderEvent::ConfirmReleased => {
                        // 在弹窗状态下，确认键释放时关闭 pop-up，避免按下和释放事件跨状态重入
                        self.state = AppState::Menu;
                        self.selected = self.menu_items
                            .iter()
                            .position(|item| item.is_settings())
                            .unwrap_or(self.selected);
                    }
                    EncoderEvent::BackPressed => {
                        // 返回菜单
                        self.state = AppState::Menu;
                        self.selected = self.menu_items
                            .iter()
                            .position(|item| item.is_settings())
                            .unwrap_or(self.selected);
                    }
                    _ => {}
                }
            }
            AppState::BleKeyboardMode => {
                match event {
                    EncoderEvent::BackPressed => {
                        // 返回菜单
                        self.state = AppState::Menu;
                        self.selected = self.menu_items
                            .iter()
                            .position(|item| item.is_ble_keyboard())
                            .unwrap_or(self.selected);
                    }
                    _ => {
                        // BLE 键盘模式下，旋转事件会发送到 BLE HID 处理
                    }
                }
            }
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        match self.state {
            AppState::Menu | AppState::BrightnessPopup => {
                // 总是先渲染菜单
                self.render_menu(frame);
                // 如果处于 pop-up 状态，在菜单上层渲染 pop-up
                if self.state == AppState::BrightnessPopup {
                    self.render_brightness_popup(frame);
                }
            }
            AppState::BleKeyboardMode => {
                self.render_ble_keyboard_mode(frame);
            }
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
                ListItem::new(item.label()).style(style)
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

    fn render_brightness_popup(&self, frame: &mut Frame) {
        use ratatui::layout::Rect;
        
        // 计算弹出窗口的居中位置，宽度 50 列，高度 3 行
        let area = frame.area();
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 3;
        
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;
        
        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup_area);
        
        // 用 Block 渲染背景边框和进度条
        let gauge = Gauge::default()
            .ratio(self.brightness as f64 / 100.0)
            .label(alloc::format!("Brightness: {}/100", self.brightness))
            .style(Style::default().fg(Color::Green).bg(Color::Black))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center)
                    .title("Settings")
                    .style(Style::default().bg(Color::Black)),
            );
        
        frame.render_widget(gauge, popup_area);
    }
    
    fn render_ble_keyboard_mode(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(frame.area());

        // 标题栏
        let title = Paragraph::new("BLE Keyboard Mode")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Blue))
            .alignment(Alignment::Center);

        frame.render_widget(title, chunks[0]);

        // 主要内容区域
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

        // 帮助提示
        let hint = Paragraph::new("Rotate: Send keys | Back: Exit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
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
