//! BLE HID 键盘模块
//! 
//! 使用 trouble 库实现 BLE HID 键盘功能，支持上下键（对应旋转编码器）

use bt_hci::controller::ExternalController;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use esp_radio::ble::controller::BleConnector;
use rtt_target::rprintln;
use trouble_host::prelude::*;

/// HID 键盘事件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleKeyEvent {
    Up,
    Down,
}

/// BLE 键盘事件通道
pub static BLE_KEY_CHANNEL: Channel<CriticalSectionRawMutex, BleKeyEvent, 8> = Channel::new();

/// USB HID 键盘键码
pub mod key_codes {
    pub const KEY_UP: u8 = 0x52;    // 上箭头
    pub const KEY_DOWN: u8 = 0x51;  // 下箭头
}

/// BLE HID 键盘任务
/// 
/// 注意：这是一个简化的实现。完整的 HID GATT 服务器实现需要：
/// 1. 正确定义 HID Service (0x1812) 及其特征
/// 2. 实现 Report Map 描述符
/// 3. 处理 GATT 读写请求
/// 
/// 由于 trouble-host 0.5 的 API 变化较大，这里提供一个框架实现。
/// 完整实现需要参考 trouble-host 的示例代码。
#[embassy_executor::task]
pub async fn ble_hid_task(
    controller: &'static esp_radio::Controller<'static>,
    bt: esp_hal::peripherals::BT<'static>,
) {
    rprintln!("BLE HID 任务启动...");

    // 创建 BLE 连接器
    let config = esp_radio::ble::Config::default();
    let transport = match BleConnector::new(controller, bt, config) {
        Ok(t) => t,
        Err(e) => {
            rprintln!("BLE 连接器创建失败: {:?}", e);
            return;
        }
    };

    // 创建外部控制器
    let ble_controller = ExternalController::<_, 20>::new(transport);

    rprintln!("BLE 控制器已创建，准备初始化协议栈...");
    
    // TODO: 完整的 HID GATT 服务器实现
    // 
    // 由于 trouble-host 0.5 的 API 变化，完整的实现需要：
    // 1. 创建 HostResources 和 Stack
    //    let mut resources: HostResources<DefaultPacketPool, 1, 1> = HostResources::new();
    //    let stack = trouble_host::new(ble_controller, &mut resources);
    //
    // 2. 创建 GATT 服务器并定义 HID Service
    //    - HID Service UUID: 0x1812
    //    - HID Information (0x2A4A): [0x01, 0x01, 0x00, 0x02] (v1.1, Not localized, Normally Connectable)
    //    - Report Map (0x2A4B): HID 报告描述符
    //    - Input Report (0x2A4D): 键盘输入报告（可通知）
    //    - Protocol Mode (0x2A4E): 0x01 (Report mode)
    //
    // 3. 设置广播数据
    //    - Appearance: 0x03C1 (Keyboard)
    //    - Service UUID: 0x1812
    //
    // 4. 处理连接事件和键盘输入
    //    - 在 BLE 键盘模式下，将旋转编码器事件转换为 HID 报告
    //    - 顺时针 -> 下箭头 (0x51)
    //    - 逆时针 -> 上箭头 (0x52)

    rprintln!("BLE HID 框架已启动（完整实现待完善）");

    // 简单的循环，接收键盘事件并打印（用于调试）
    loop {
        let event = BLE_KEY_CHANNEL.receive().await;
        match event {
            BleKeyEvent::Up => rprintln!("BLE 按键: 上箭头 (0x{:02X})", key_codes::KEY_UP),
            BleKeyEvent::Down => rprintln!("BLE 按键: 下箭头 (0x{:02X})", key_codes::KEY_DOWN),
        }
    }
}

/// HID 报告描述符（标准键盘）
#[allow(dead_code)]
pub const HID_REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop Ctrls)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)
    
    // Modifier 字节
    0x05, 0x07, // Usage Page (Kbrd/Keypad)
    0x19, 0xE0, // Usage Minimum (Left Control)
    0x29, 0xE7, // Usage Maximum (Right GUI)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0x01, // Logical Maximum (1)
    0x75, 0x01, // Report Size (1 bit)
    0x95, 0x08, // Report Count (8 bits)
    0x81, 0x02, // Input (Data, Var, Abs)
    
    // Reserved 字节
    0x95, 0x01, // Report Count (1)
    0x75, 0x08, // Report Size (8)
    0x81, 0x01, // Input (Cnst, Var, Abs)
    
    // 按键码（最多 6 个同时按键）
    0x95, 0x06, // Report Count (6)
    0x75, 0x08, // Report Size (8)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0x65, // Logical Maximum (101)
    0x05, 0x07, // Usage Page (Kbrd/Keypad)
    0x19, 0x00, // Usage Minimum (0)
    0x29, 0x65, // Usage Maximum (101)
    0x81, 0x00, // Input (Data, Array, Abs)
    
    0xC0        // End Collection
];

/// 键盘输入报告（8 字节）
#[derive(Debug, Clone, Copy)]
pub struct KeyboardReport {
    pub modifier: u8,
    pub reserved: u8,
    pub keys: [u8; 6],
}

impl KeyboardReport {
    pub const EMPTY: Self = Self {
        modifier: 0,
        reserved: 0,
        keys: [0; 6],
    };

    pub fn with_key(key: u8) -> Self {
        let mut report = Self::EMPTY;
        report.keys[0] = key;
        report
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        [
            self.modifier,
            self.reserved,
            self.keys[0],
            self.keys[1],
            self.keys[2],
            self.keys[3],
            self.keys[4],
            self.keys[5],
        ]
    }
}
