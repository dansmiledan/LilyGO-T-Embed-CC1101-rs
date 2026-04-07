//! BLE HID 键盘模块
//! 
//! 使用 trouble-host 库实现真实的 BLE HID 键盘功能

use bt_hci::controller::Controller;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_futures::{join::join, select::select};
use rtt_target::rprintln;
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

/// GATT 服务器定义 - HID 键盘
#[gatt_server]
struct Server {
    hid_service: HidService,
}

/// HID 服务 (UUID 0x180A)
#[gatt_service(uuid = "180A")]
struct HidService {
    /// HID 输入报告 (UUID 0x2A4D) - 8 字节键盘报告
    #[characteristic(uuid = "2A4D", read, write_without_response, notify)]
    input_report: [u8; 8],
}

/// BLE 主控制器初始化和运行
#[embassy_executor::task]
pub async fn run_ble_keyboard(bluetooth:esp_hal::peripherals::BT<'static> ) {
    rprintln!("🔌 启动 BLE 键盘服务...");
    
    let address: Address = Address::random([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]);
    rprintln!("📍 BLE 地址: {:?}", address);

    let connector = BleConnector::new(bluetooth, Default::default()).unwrap();
    let controller: ExternalController<_, 1> = ExternalController::new(connector);
    let mut resources: HostResources<DefaultPacketPool, 1, 2> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);
    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    rprintln!("✓ BLE 主机已创建");

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "T-Embed-KB",
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
    }))
    .unwrap();

    let _ = join(ble_task(runner), async {
        loop {
            match advertise("T-Embed-KB", &mut peripheral, &server).await {
                Ok(conn) => {
                    rprintln!("✅ 客户端已连接");
                    let gatt = gatt_events_task(&server, &conn);
                    let keyboard = keyboard_task(&server, &conn);
                    select(gatt, keyboard).await;
                    rprintln!("↻ 等待新连接...");
                }
                Err(e) => {
                    rprintln!("❌ 广告错误: {:?}", e);
                }
            }
        }
    })
    .await;
}

/// BLE 控制器循环
async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    rprintln!("🔄 BLE 事件循环已启动");
    loop {
        if let Err(e) = runner.run().await {
            rprintln!("❌ BLE 错误: {:?}", e);
        }
    }
}

/// GATT 事件处理
async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    rprintln!("📡 GATT 监听已启动");
    let input_report = server.hid_service.input_report;

    let reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                match &event {
                    GattEvent::Read(event) => {
                        if event.handle() == input_report.handle {
                            let value = server.get(&input_report);
                            rprintln!("📖 读取输入报告: {:02X?}", value);
                        }
                    }
                    GattEvent::Write(event) => {
                        if event.handle() == input_report.handle {
                            rprintln!("✏️ 写入输入报告: {:02X?}", event.data());
                        }
                    }
                    _ => {}
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => rprintln!("❌ GATT 应答错误: {:?}", e),
                };
            }
            _ => {}
        }
    };
    rprintln!("🔌 连接已断开: {:?}", reason);
    Ok(())
}

/// 键盘事件处理任务
async fn keyboard_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    rprintln!("⌨️ 键盘监听已启动");
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
                    BleKeyEvent::Up => (key_codes::KEY_UP, "↑ UP"),
                    BleKeyEvent::Down => (key_codes::KEY_DOWN, "↓ DOWN"),
                };

                // 按键按下
                let report = KeyboardReport::with_key(key_code);
                let bytes = report.to_bytes();
                
                rprintln!("[{}] 🔑 按键: {}", counter, key_name);
                if input_report.notify(conn, &bytes).await.is_err() {
                    rprintln!("❌ 发送键按下失败");
                    break;
                }

                // 延迟后释放键
                embassy_time::Timer::after(embassy_time::Duration::from_millis(50)).await;
                let release_bytes = KeyboardReport::EMPTY.to_bytes();
                if input_report.notify(conn, &release_bytes).await.is_err() {
                    rprintln!("❌ 发送键释放失败");
                    break;
                }
                rprintln!("✓ 键已释放");
            }
            Err(_) => {
                // 超时，继续等待
            }
        }
    }
}

/// 广告和连接
async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut advertiser_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[[0x0A, 0x18]]), // HID Service UUID
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut advertiser_data[..],
    )?;

    rprintln!("📢 开始广告: {}", name);
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..len],
                scan_data: &[],
            },
        )
        .await?;

    rprintln!("⏳ 等待连接...");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}
