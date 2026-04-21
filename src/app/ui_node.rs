use crate::app::{Command, NodeResult};
use crate::input::EncoderEvent;
use ratatui::prelude::*;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Clear, Gauge, List, ListItem, Paragraph};

/// UI 节点接口
/// 每个界面（菜单、功能页）实现此 trait
/// 节点之间通过栈管理，形成层级导航结构
pub trait UiNode: core::fmt::Debug {
    /// 节点标签（菜单中显示的名称）
    #[allow(dead_code)]
    fn label(&self) -> &'static str;

    /// 进入节点时调用（可以用于启动后台任务等）
    fn on_enter(&mut self) -> alloc::vec::Vec<Command> {
        alloc::vec::Vec::new()
    }

    /// 退出节点时调用（可以用于停止后台任务等）
    fn on_exit(&mut self) -> alloc::vec::Vec<Command> {
        alloc::vec::Vec::new()
    }

    /// 处理编码器事件
    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>);

    /// 渲染节点界面
    fn render(&self, frame: &mut Frame<'_>, area: Rect);
}

// =============================================================================
// 渲染辅助函数
// =============================================================================

/// 通用菜单渲染
pub fn render_menu_screen(
    frame: &mut Frame<'_>,
    title: &str,
    items: &[&str],
    selected: usize,
    hint: &str,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // 标题栏
    let title_widget = Paragraph::new(title)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title_alignment(Alignment::Center),
        )
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .alignment(Alignment::Center);
    frame.render_widget(title_widget, chunks[0]);

    // 菜单列表
    let list_items: alloc::vec::Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == selected {
                Style::default().fg(Color::Black).bg(Color::White).bold()
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(*label).style(style)
        })
        .collect();

    let list = List::new(list_items)
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
    let hint_widget = Paragraph::new(hint)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint_widget, chunks[2]);
}

/// 渲染亮度弹窗
pub fn render_brightness_popup(frame: &mut Frame<'_>, brightness: u8) {
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

    let gauge = Gauge::default()
        .ratio(brightness as f64 / 100.0)
        .label(alloc::format!("Brightness: {}/100", brightness))
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
