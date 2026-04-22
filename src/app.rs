use alloc::boxed::Box;
use crate::ble_hid::{BleControlEvent, BleKeyEvent};
use crate::input::EncoderEvent;
use ratatui::Frame;

pub mod ui_node;
pub mod root_menu;
pub mod ble;
pub mod settings;
pub mod sd_browser;

pub use ui_node::UiNode;

// =============================================================================
// 硬件命令
// =============================================================================

/// 硬件操作命令
/// App 处理完事件后，通过命令通知 main 执行具体的硬件操作
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// 设置背光亮度 (0-100)
    Backlight(u8),
    /// BLE 控制（启动/停止广播）
    BleControl(BleControlEvent),
    /// BLE HID 按键事件
    BleKey(BleKeyEvent),
}

// =============================================================================
// 节点结果
// =============================================================================

/// UI 节点事件处理结果
/// `handle_event` 返回节点导航结果
#[derive(Debug)]
pub enum NodeResult {
    /// 留在当前节点
    Stay,
    /// 压入新节点（进入子菜单或功能）
    Push(Box<dyn UiNode>),
    /// 弹出当前节点（返回父节点）
    Pop,
}

// =============================================================================
// 导航栈
// =============================================================================

/// 层级导航栈
/// 维护当前 UI 节点的栈，支持压栈（进入子界面）和弹栈（返回上级）
pub struct NavigationStack {
    stack: alloc::vec::Vec<Box<dyn UiNode>>,
}

impl NavigationStack {
    pub fn new(root: Box<dyn UiNode>) -> Self {
        Self {
            stack: alloc::vec![root],
        }
    }

    pub fn current(&self) -> &dyn UiNode {
        self.stack.last().unwrap().as_ref()
    }

    pub fn current_mut(&mut self) -> &mut dyn UiNode {
        self.stack.last_mut().unwrap().as_mut()
    }

    /// 压入新节点，返回 on_enter 产生的命令
    pub fn push(&mut self, mut node: Box<dyn UiNode>) -> alloc::vec::Vec<Command> {
        let cmds = node.on_enter();
        self.stack.push(node);
        cmds
    }

    /// 弹出当前节点，返回 on_exit 产生的命令
    pub fn pop(&mut self) -> alloc::vec::Vec<Command> {
        if self.stack.len() > 1 {
            let mut node = self.stack.pop().unwrap();
            node.on_exit()
        } else {
            alloc::vec::Vec::new()
        }
    }

    #[allow(dead_code)]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

// =============================================================================
// 应用状态机
// =============================================================================

/// 应用主结构
/// 持有导航栈，协调事件处理和渲染
pub struct App {
    nav: NavigationStack,
}

impl App {
    pub fn new() -> Self {
        let root = Box::new(root_menu::RootMenu::new());
        let mut nav = NavigationStack::new(root);
        // 触发根节点的 on_enter
        let _ = nav.current_mut().on_enter();
        Self { nav }
    }

    /// 处理编码器事件，返回需要执行的硬件命令
    pub fn handle_event(&mut self, event: EncoderEvent) -> alloc::vec::Vec<Command> {
        let mut commands = alloc::vec::Vec::new();

        let (result, event_cmds) = self.nav.current_mut().handle_event(event);
        commands.extend(event_cmds);

        match result {
            NodeResult::Stay => {}
            NodeResult::Push(node) => {
                commands.extend(self.nav.push(node));
            }
            NodeResult::Pop => {
                commands.extend(self.nav.pop());
            }
        }

        commands
    }

    /// 定期更新（每帧调用）
    pub fn tick(&mut self) -> alloc::vec::Vec<Command> {
        alloc::vec::Vec::new()
    }

    pub fn render(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        self.nav.current().render(frame, area);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
