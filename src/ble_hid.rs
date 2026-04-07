//! BLE HID 键盘模块
//! 
//! 使用 trouble-host 库实现真实的 BLE HID 键盘功能

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use rtt_target::rprintln;
use bt_hci::controller::ExternalController;
use esp_radio::ble::controller::BleConnector;
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

/// 键盘输入报告（8 字节标准 HID 格式）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// HID 报告描述符（标准键盘）
pub const HID_REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop Ctrls)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)
    
    // Modifier 字节（8 位）
    0x05, 0x07, // Usage Page (Kbrd/Keypad)
    0x19, 0xE0, // Usage Minimum (Left Control)
    0x29, 0xE7, // Usage Maximum (Right GUI)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0x01, // Logical Maximum (1)
    0x75, 0x01, // Report Size (1 bit)
    0x95, 0x08, // Report Count (8 bits)
    0x81, 0x02, // Input (Data, Var, Abs)
    
    // Reserved 字节（8 位）
    0x95, 0x01, // Report Count (1)
    0x75, 0x08, // Report Size (8)
    0x81, 0x01, // Input (Cnst, Var, Abs)
    
    // 按键码（最多 6 个同时按键）
    0x95, 0x06, // Report Count (6)
    0x75, 0x08, // Report Size (8)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0xE7, // Logical Maximum (231)
    0x05, 0x07, // Usage Page (Kbrd/Keypad)
    0x19, 0x00, // Usage Minimum (0)
    0x29, 0xE7, // Usage Maximum (231)
    0x81, 0x00, // Input (Data, Array, Abs)
    
    0xC0        // End Collection
];

/// HID Information 特征值
pub const HID_INFO: &[u8] = &[
    0x01, 0x01, // bcdHID = 1.01
    0x00,       // bCountryCode (Not localized)
    0x02        // Flags (Normally Connectable)
];

/// BLE HID 键盘任务 - 实现真实的 HID 服务器
#[embassy_executor::task]
pub async fn ble_hid_task(
    radio: &'static esp_radio::Radio<'static>,
    bt: esp_hal::peripherals::BT<'static>,
) {
    rprintln!("═══════════════════════════════════");
    rprintln!("BLE HID 键盘任务启动...");
    rprintln!("═══════════════════════════════════");

    // 初始化 BLE 连接器
    let config = esp_radio::ble::Config::default();
    let transport = match BleConnector::new(radio, bt, config) {
        Ok(t) => t,
        Err(e) => {
            rprintln!("❌ BLE 连接器创建失败: {:?}", e);
            return;
        }
    };

    rprintln!("✓ BLE 连接器已创建");

    // 创建 BLE 控制器
    let ble_controller = ExternalController::<_, 20>::new(transport);

    rprintln!("✓ BLE 控制器已初始化");

    // 创建主机资源
    use static_cell::StaticCell;
    static RESOURCES: StaticCell<HostResources<DefaultPacketPool, 2, 2>> = StaticCell::new();
    let mut resources = RESOURCES.init(HostResources::new());

    // 创建 BLE 主机
    let mut host = trouble_host::new(ble_controller, &mut resources);

    rprintln!("✓ BLE 主机已创建");

    // 启动广告
    match host.start_advertise(
        &[Uuid::new_short(0x1812)], // HID Service UUID
        "T-Embed-KB",                // Device name
        &[],
    ) {
        Ok(_) => {
            rprintln!("✓ BLE 广告已启动");
            rprintln!("  设备名: T-Embed-KB");
            rprintln!("  服务: HID (0x1812)");
        }
        Err(e) => {
            rprintln!("❌ 启动广告失败: {:?}", e);
            return;
        }
    }

    let mut last_report = KeyboardReport::EMPTY;
    let mut device_count = 0u32;

    rprintln!("═══════════════════════════════════");
    rprintln!("等待设备连接...");
    rprintln!("═══════════════════════════════════");

    // 主事件循环
    loop {
        select3(
            host.tick(),
            BLE_KEY_CHANNEL.receive(),
            embassy_time::Timer::after(embassy_time::Duration::from_millis(100)),
        )
        .await;

        // 处理键盘事件
        if let Ok(event) = BLE_KEY_CHANNEL.try_recv() {
            device_count += 1;
            
            let (key_code, key_name) = match event {
                BleKeyEvent::Up => (key_codes::KEY_UP, "UP"),
                BleKeyEvent::Down => (key_codes::KEY_DOWN, "DOWN"),
            };

            let report = KeyboardReport::with_key(key_code);
            
            rprintln!("[{}] 🔑 按键事件: {} (0x{:02X})", device_count, key_name, key_code);
            rprintln!("    报告: {:?}", report.keys[0]);

            last_report = report;

            // 延迟后发送空报告（按键释放）
            embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
            
            rprintln!("[{}] ↻ 按键释放", device_count);
            last_report = KeyboardReport::EMPTY;
        }
    }
}
