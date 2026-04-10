//! BLE HID 键盘模块
//! 
//! 使用 trouble-host 库实现标准的 BLE HID 键盘功能
//! 符合 HID over GATT Profile (HOGP) 规范

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_futures::{join::join, select::select};
use log::info;
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

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

/// HID Report Descriptor for a standard keyboard
/// 定义了一个标准的 8 字节键盘报告格式
// HID Report Descriptor (63 bytes) - 已内联到 GATT 特征定义中

/// GATT 服务器定义 - HID 键盘
#[gatt_server]
#[allow(dead_code)]
struct Server {
    /// HID Service (UUID 0x1812)
    hid_service: HidService,
    /// Battery Service (UUID 0x180F) - 可选但推荐
    battery_service: BatteryService,
    /// Device Information Service (UUID 0x180A) - 可选但推荐
    device_info_service: DeviceInfoService,
}

/// HID Service (UUID 0x1812)
#[gatt_service(uuid = "1812")]
struct HidService {
    /// HID Information (UUID 0x2A4A) - 必须
    /// 包含 HID 版本、国家代码和标志
    /// 值: [0x11, 0x01, 0x00, 0x02] = v1.1, 国家代码0, 标志(正常可连接+远程唤醒)
    #[characteristic(uuid = "2A4A", read, value = [0x11, 0x01, 0x00, 0x02])]
    hid_info: [u8; 4],
    
    /// Report Map (UUID 0x2A4B) - 必须
    /// 定义 HID 报告格式
    #[characteristic(uuid = "2A4B", read, value = [
        0x05, 0x01,       // Usage Page (Generic Desktop)
        0x09, 0x06,       // Usage (Keyboard)
        0xA1, 0x01,       // Collection (Application)
        0x05, 0x07,       //   Usage Page (Key Codes)
        0x19, 0xE0,       //   Usage Minimum (224)
        0x29, 0xE7,       //   Usage Maximum (231)
        0x15, 0x00,       //   Logical Minimum (0)
        0x25, 0x01,       //   Logical Maximum (1)
        0x75, 0x01,       //   Report Size (1)
        0x95, 0x08,       //   Report Count (8)
        0x81, 0x02,       //   Input (Data, Variable, Absolute)
        0x95, 0x01,       //   Report Count (1)
        0x75, 0x08,       //   Report Size (8)
        0x81, 0x01,       //   Input (Constant)
        0x95, 0x05,       //   Report Count (5)
        0x75, 0x01,       //   Report Size (1)
        0x05, 0x08,       //   Usage Page (LEDs)
        0x19, 0x01,       //   Usage Minimum (1)
        0x29, 0x05,       //   Usage Maximum (5)
        0x91, 0x02,       //   Output (Data, Variable, Absolute)
        0x95, 0x01,       //   Report Count (1)
        0x75, 0x03,       //   Report Size (3)
        0x91, 0x01,       //   Output (Constant)
        0x95, 0x06,       //   Report Count (6)
        0x75, 0x08,       //   Report Size (8)
        0x15, 0x00,       //   Logical Minimum (0)
        0x25, 0x65,       //   Logical Maximum (101)
        0x05, 0x07,       //   Usage Page (Key Codes)
        0x19, 0x00,       //   Usage Minimum (0)
        0x29, 0x65,       //   Usage Maximum (101)
        0x81, 0x00,       //   Input (Data, Array)
        0xC0              // End Collection
    ])]
    report_map: [u8; 63],
    
    /// HID Control Point (UUID 0x2A4C) - 必须
    /// 用于控制 HID 行为（挂起/恢复）
    /// 0x00 = 挂起, 0x01 = 退出挂起
    #[characteristic(uuid = "2A4C", write_without_response, value = 0x00)]
    hid_control_point: u8,
    
    /// Protocol Mode (UUID 0x2A4E) - 必须
    /// 0 = Boot Protocol, 1 = Report Protocol
    #[characteristic(uuid = "2A4E", read, write_without_response, value = 0x01)]
    protocol_mode: u8,
    
    /// HID Input Report (UUID 0x2A4D) - 必须
    /// 键盘输入报告，8字节标准格式
    #[characteristic(uuid = "2A4D", read, notify, value = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[descriptor(uuid = "2908", read, value = [0x00, 0x01])]  // Report Reference: Report ID=0, Type=Input(1)
    input_report: [u8; 8],
    
    /// HID Output Report (UUID 0x2A4D) - 可选
    /// LED 控制报告（Caps Lock, Num Lock 等）
    #[characteristic(uuid = "2A4D", read, write, write_without_response, value = [0x00])]
    #[descriptor(uuid = "2908", read, value = [0x00, 0x02])]  // Report Reference: Report ID=0, Type=Output(2)
    output_report: [u8; 1],
}

/// Battery Service (UUID 0x180F)
#[gatt_service(uuid = "180F")]
struct BatteryService {
    /// Battery Level (UUID 0x2A19)
    #[characteristic(uuid = "2A19", read, notify, value = 100)]
    battery_level: u8,
}

/// Device Information Service (UUID 0x180A)
#[gatt_service(uuid = "180A")]
struct DeviceInfoService {
    /// Manufacturer Name (UUID 0x2A29)
    #[characteristic(uuid = "2A29", read, value = [b'L', b'i', b'l', b'y', b'G', b'o'])]
    manufacturer: [u8; 6],
    /// Model Number (UUID 0x2A24)
    #[characteristic(uuid = "2A24", read, value = [b'T', b'-', b'E', b'm', b'b', b'e', b'd'])]
    model_number: [u8; 7],
    /// PnP ID (UUID 0x2A50)
    /// Vendor ID Source (1=Bluetooth SIG), Vendor ID, Product ID, Product Version
    #[characteristic(uuid = "2A50", read, value = [0x01, 0xE5, 0x02, 0x00, 0x01, 0x00, 0x01])]
    pnp_id: [u8; 7],
}

/// BLE 主控制器初始化和运行
#[embassy_executor::task]
pub async fn run_ble_keyboard(bluetooth: esp_hal::peripherals::BT<'static>) {
    info!("启动 BLE 键盘服务...");
    
    // 使用静态随机地址（Static Random Address）
    // 最低两位必须是 11b 表示静态随机地址
    let address: Address = Address::random([0xC3, 0x45, 0x67, 0x89, 0xAB, 0xCD]);
    info!("BLE 地址: {:?}", address);

    let connector = BleConnector::new(bluetooth, Default::default()).unwrap();
    let controller: ExternalController<_, 1> = ExternalController::new(connector);
    let mut resources: HostResources<DefaultPacketPool, 1, 8> = HostResources::new();
    
    // 创建随机数生成器（用于安全配对）
    let mut rng = ChaCha8Rng::from_seed([0x42; 32]);
    
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .set_random_generator_seed(&mut rng);
    
    // 设置 IO 能力：键盘设备使用 NoInputNoOutput（无显示无输入）
    // 这样配对时使用 Just Works 模式
    stack.set_io_capabilities(IoCapabilities::NoInputNoOutput);
    
    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    info!("✓ BLE 主机已创建（安全配对已启用）");

    // 创建 GATT 服务器，配置为 HID 键盘设备
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "T-Embed-KB",
        // 设置为键盘外观 (0x03C1)
        appearance: &appearance::human_interface_device::KEYBOARD,
    }))
    .unwrap();

    info!("✓ GATT 服务器已创建 (HID Keyboard)");

    let _ = join(
        async {
            info!("BLE 事件循环已启动");
            let mut runner = runner;
            loop {
                if let Err(e) = runner.run().await {
                    info!("BLE 错误: {:?}", e);
                }
            }
        },
        async {
            loop {
                match advertise("T-Embed-KB", &mut peripheral, &server).await {
                    Ok(conn) => {
                        info!("客户端已连接");
                        let gatt = gatt_events_task(&server, &conn);
                        let keyboard = keyboard_task(&server, &conn);
                        select(gatt, keyboard).await;
                        info!("等待新连接...");
                    }
                    Err(e) => {
                        info!("广告错误: {:?}", e);
                    }
                }
            }
        }
    )
    .await;
}

/// GATT 事件处理
async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    info!("GATT 监听已启动");
    let input_report = server.hid_service.input_report;
    let output_report = server.hid_service.output_report;

    let reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                match &event {
                    GattEvent::Read(event) => {
                        if event.handle() == input_report.handle {
                            let value = server.get(&input_report);
                            info!("读取输入报告: {:02X?}", value);
                        } else if event.handle() == output_report.handle {
                            let value = server.get(&output_report);
                            info!("读取输出报告: {:02X?}", value);
                        }
                    }
                    GattEvent::Write(event) => {
                        if event.handle() == server.hid_service.hid_control_point.handle {
                            info!("HID Control Point 写入: {:02X?}", event.data());
                        } else if event.handle() == server.hid_service.protocol_mode.handle {
                            info!("Protocol Mode 写入: {:02X?}", event.data());
                            // 注意：write_without_response 的特征值会自动更新，无需手动设置
                        } else if event.handle() == input_report.handle {
                            info!("写入输入报告: {:02X?}", event.data());
                        } else if event.handle() == output_report.handle {
                            let data = event.data();
                            info!("LED 输出报告写入: {:02X?}", data);
                            // LED 状态：bit0=Num Lock, bit1=Caps Lock, bit2=Scroll Lock
                            if let Some(leds) = data.first() {
                                info!("LED 状态: NumLock={} CapsLock={} ScrollLock={}",
                                    (leds >> 0) & 1, (leds >> 1) & 1, (leds >> 2) & 1);
                                // 特征值会自动更新，无需手动设置
                            }
                        }
                    }
                    _ => {}
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => info!("GATT 应答错误: {:?}", e),
                };
            }
            _ => {}
        }
    };
    info!("连接已断开: {:?}", reason);
    Ok(())
}

/// 键盘事件处理任务
async fn keyboard_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    info!("键盘监听已启动，等待按键事件...");
    let input_report = server.hid_service.input_report;
    let mut counter = 0u32;

    loop {
        // 接收键盘事件（带超时防止完全阻塞）
        match embassy_time::with_timeout(
            embassy_time::Duration::from_millis(500),
            BLE_KEY_CHANNEL.receive()
        ).await {
            Ok(event) => {
                counter += 1;
                let (key_code, key_name) = match event {
                    BleKeyEvent::Up => (key_codes::KEY_UP, "UP"),
                    BleKeyEvent::Down => (key_codes::KEY_DOWN, "DOWN"),
                };

                // 按键按下
                let report = KeyboardReport::with_key(key_code);
                let bytes = report.to_bytes();
                
                info!("[{}] 按键: {} (0x{:02X}) -> {:?}", counter, key_name, key_code, bytes);
                
                // 发送按键按下通知
                match input_report.notify(conn, &bytes).await {
                    Ok(_) => info!("  -> 按键已发送"),
                    Err(e) => {
                        info!("  -> 发送失败: {:?}", e);
                        break;
                    }
                }

                // 延迟后释放键（50ms 更可靠）
                embassy_time::Timer::after(embassy_time::Duration::from_millis(50)).await;
                let release_bytes = KeyboardReport::EMPTY.to_bytes();
                match input_report.notify(conn, &release_bytes).await {
                    Ok(_) => info!("  -> 释放已发送"),
                    Err(e) => {
                        info!("  -> 释放失败: {:?}", e);
                        break;
                    }
                }
            }
            Err(_) => {
                // 超时，继续等待
            }
        }
    }
    info!("键盘任务已退出");
}

/// 广告和连接
async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    // 广告数据：Flags + HID Service UUID + Appearance
    let mut advertiser_data = [0; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            // HID Service UUID (0x1812) - 小端序
            AdStructure::ServiceUuids16(&[[0x12, 0x18]]),

        ],
        &mut advertiser_data[..],
    )?;

    // 扫描响应数据：完整设备名称
    let mut scan_data = [0; 31];
    let scan_len = AdStructure::encode_slice(
        &[
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut scan_data[..],
    )?;

    info!("开始广告: {} (HID Keyboard)", name);
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..adv_len],
                scan_data: &scan_data[..scan_len],
            },
        )
        .await?;

    info!("等待连接...");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}
