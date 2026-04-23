use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::ui_node::UiNode;
use crate::app::{Command, NodeResult};
use crate::input::EncoderEvent;
use crate::sd_card::{list_directory, DirEntryInfo, format_size};

// =============================================================================
// 文件浏览器节点
// =============================================================================

#[derive(Debug)]
pub struct SdBrowserNode {
    /// 当前目录路径（短文件名路径，用于文件系统操作）
    current_path: String,
    /// 路径栈，保存每一级目录的显示名（长文件名），用于标题栏
    path_stack: Vec<String>,
    /// 缓存的目录条目
    entries: Vec<DirEntryInfo>,
    /// 当前选中项索引
    selected: usize,
    /// 是否有错误
    error_msg: Option<String>,
    /// 滚动偏移（用于长列表）
    scroll_offset: usize,
    /// 缓存上次渲染时计算出的可见行数，供事件处理使用
    cached_visible_rows: Cell<usize>,
}

impl SdBrowserNode {
    pub fn new() -> Self {
        Self {
            current_path: String::new(),
            path_stack: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            error_msg: None,
            scroll_offset: 0,
            cached_visible_rows: Cell::new(5),
        }
    }

    /// 刷新当前目录内容
    fn refresh(&mut self) {
        match list_directory(&self.current_path) {
            Ok(mut entries) => {
                // 目录排在前面，文件排在后面，各自按字母序排序
                entries.sort_by(|a, b| {
                    match (a.is_dir, b.is_dir) {
                        (true, false) => core::cmp::Ordering::Less,
                        (false, true) => core::cmp::Ordering::Greater,
                        _ => a.name.cmp(&b.name),
                    }
                });
                self.entries = entries;
                self.selected = 0;
                self.scroll_offset = 0;
                self.error_msg = None;
            }
            Err(e) => {
                self.entries.clear();
                self.selected = 0;
                self.error_msg = Some(format!("{:?}", e));
            }
        }
    }



    /// 计算可见区域能显示的行数
    fn visible_rows(&self, area_height: u16) -> usize {
        // 标题占3行，底部提示占1行，边框等
        let content_height = area_height.saturating_sub(4);
        (content_height as usize).max(1)
    }

    /// 更新滚动偏移，确保选中项在可视区域内
    fn update_scroll(&mut self, visible_rows: usize) {
        if self.selected >= self.scroll_offset + visible_rows {
            self.scroll_offset = self.selected.saturating_sub(visible_rows - 1);
        } else if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }
}

impl UiNode for SdBrowserNode {
    fn label(&self) -> &'static str {
        "SD Card"
    }

    fn on_enter(&mut self) -> alloc::vec::Vec<Command> {
        self.refresh();
        alloc::vec::Vec::new()
    }

    fn handle_event(&mut self, event: EncoderEvent) -> (NodeResult, alloc::vec::Vec<Command>) {
        match event {
            EncoderEvent::Clockwise => {
                if !self.entries.is_empty() && self.selected < self.entries.len() - 1 {
                    self.selected += 1;
                }
                let visible_rows = self.cached_visible_rows.get();
                self.update_scroll(visible_rows);
            }
            EncoderEvent::CounterClockwise => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                let visible_rows = self.cached_visible_rows.get();
                self.update_scroll(visible_rows);
            }
            EncoderEvent::ConfirmReleased => {
                if let Some(entry) = self.entries.get(self.selected) {
                    if entry.is_dir {
                        // 进入子目录：路径用短文件名，显示名用长文件名
                        if self.current_path.is_empty() {
                            self.current_path = entry.short_name.clone();
                        } else {
                            self.current_path = format!("{}/{}", self.current_path, entry.short_name);
                        }
                        self.path_stack.push(entry.name.clone());
                        self.refresh();
                    } else {
                        // 选中文件，暂不做特殊操作（未来可扩展为查看文件内容）
                    }
                }
            }
            EncoderEvent::BackPressed => {
                if self.current_path.is_empty() {
                    // 在根目录，退出文件浏览器
                    return (NodeResult::Pop, alloc::vec::Vec::new());
                } else {
                    // 返回上级目录
                    if let Some(pos) = self.current_path.rfind('/') {
                        self.current_path.truncate(pos);
                    } else {
                        self.current_path.clear();
                    }
                    self.path_stack.pop();
                    self.refresh();
                }
            }
            _ => {}
        }
        (NodeResult::Stay, alloc::vec::Vec::new())
    }

    fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        let area = frame.area();
        let visible_rows = self.visible_rows(area.height);
        self.cached_visible_rows.set(visible_rows);

        let chunks = Layout::default()
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        // 标题栏：显示当前路径
        let path_display = if self.current_path.is_empty() {
            "/ (root)"
        } else {
            &self.current_path
        };
        let title = Paragraph::new(alloc::format!("SD Card - {}",path_display))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        // 如果有错误，显示错误信息
        if let Some(ref err) = self.error_msg {
            let err_widget = Paragraph::new(alloc::format!("Error: {}", err))
                .style(Style::default().fg(Color::Red).bg(Color::Black))
                .alignment(Alignment::Center);
            frame.render_widget(err_widget, chunks[1]);
        } else if self.entries.is_empty() {
            let empty_widget = Paragraph::new("(Empty directory)")
                .style(Style::default().fg(Color::DarkGray).bg(Color::Black))
                .alignment(Alignment::Center);
            frame.render_widget(empty_widget, chunks[1]);
        } else {
            // 文件列表
            let end_idx = (self.scroll_offset + visible_rows).min(self.entries.len());
            let visible_entries = &self.entries[self.scroll_offset..end_idx];

            // 根据列表区域宽度动态计算文件名显示宽度
            let inner_width = chunks[1].width.saturating_sub(2) as usize;
            let dir_name_width = inner_width.saturating_sub(3); // '[' '/' ']'
            let file_name_width = inner_width.saturating_sub(10); // 预留空格 + 大小

            let list_items: Vec<ListItem> = visible_entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let actual_idx = self.scroll_offset + i;
                    let label = if entry.is_dir {
                        alloc::format!("[{:<dir_width$}/]", entry.name, dir_width = dir_name_width)
                    } else {
                        alloc::format!("{:<name_width$} {}", entry.name, format_size(entry.size), name_width = file_name_width)
                    };

                    let style = if actual_idx == self.selected {
                        Style::default().fg(Color::Black).bg(Color::White).bold()
                    } else if entry.is_dir {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    ListItem::new(label).style(style)
                })
                .collect();

            let list = List::new(list_items)
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .title("Files"),
                )
                .style(Style::default().bg(Color::Black));
            frame.render_widget(list, chunks[1]);
        }

        // 底部提示
        let hint = if self.current_path.is_empty() {
            "Rotate: Navigate | Confirm: Open | Back: Exit"
        } else {
            "Rotate: Navigate | Confirm: Open | Back: Up"
        };
        let hint_widget = Paragraph::new(hint)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(hint_widget, chunks[2]);
    }
}
