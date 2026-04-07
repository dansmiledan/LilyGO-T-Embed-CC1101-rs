#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod input;
mod ui;
mod backlight;
mod ble_hid;

use embassy_executor::Spawner;
use embassy_time::Duration;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig, Pin};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::Terminal;
use rtt_target::rprintln;

use input::{init_encoder, encoder_task, ENCODER_CHANNEL, EncoderEvent};
use ui::App;
use ble_hid::{BleKeyEvent, BLE_KEY_CHANNEL};

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
	rprintln!("Panic occurred: {:?}", p);
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();
const MAX_BRIGHTNESS_STEPS: u8 = 16;

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_print!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // 初始化堆分配器
    esp_alloc::heap_allocator!(size: 128 * 1024);
    
    // esp-hal 1.0 自动初始化 PSRAM（如果启用了 psram feature）
    // 使用 psram_raw_parts 获取 PSRAM 信息
    let (psram_start, psram_size) = esp_hal::psram::psram_raw_parts(&peripherals.PSRAM);
    rprintln!("PSRAM 信息: 起始地址={:p}, 大小={} 字节", psram_start, psram_size);
    
    // 如果 PSRAM 已初始化，将其添加到堆
    if psram_size > 0 {
        unsafe {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                psram_start,
                psram_size,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }
        rprintln!("PSRAM 已添加到堆分配器");
    }

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    rprintln!("T-Embed CC1101 UI Starting...");

    // 创建编码器实例
    let encoder = init_encoder(
        peripherals.GPIO4.degrade(),
        peripherals.GPIO5.degrade(),
        peripherals.GPIO0.degrade(),
        peripherals.GPIO6.degrade(),
    );

    // 启动编码器任务
    spawner.spawn(encoder_task(encoder).unwrap());

    // // 初始化 BLE 无线电
    // rprintln!("初始化 BLE 无线电...");
    // let radio_init = match esp_radio::init() {
    //     Ok(init) => {
    //         rprintln!("BLE 无线电初始化成功");
    //         init
    //     }
    //     Err(e) => {
    //         rprintln!("BLE 无线电初始化失败: {:?}", e);
    //         panic!("BLE 初始化失败");
    //     }
    // };

    // 使用 StaticCell 来创建静态引用
    // use static_cell::StaticCell;
    // static RADIO_INIT: StaticCell<esp_radio::Controller> = StaticCell::new();
    // let radio_init_static = RADIO_INIT.init(radio_init);
    
    // 启动 BLE HID 任务
    // spawner.spawn(ble_hid_task(radio_init_static, peripherals.BT)).unwrap();
    rprintln!("BLE HID 任务已启动");

    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(SpiMode::_0);

    let spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO11)
        .with_mosi(peripherals.GPIO9);

    let cs = Output::new(peripherals.GPIO41, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let mut backlight = Output::new(peripherals.GPIO21, Level::High, OutputConfig::default());

    use display_interface_spi::SPIInterface;
    use embedded_hal_bus::spi::ExclusiveDevice;

    let spi_dev = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    let di = SPIInterface::new(spi_dev, dc);

    use mipidsi::Builder;
    use mipidsi::models::ST7789;
    use mipidsi::options::ColorOrder;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::draw_target::DrawTarget;
    use embedded_graphics::prelude::RgbColor;

    let mut display = Builder::new(ST7789, di)
        .color_order(ColorOrder::Bgr)
        .display_offset(35, 0)
        .display_size(170, 320)
        .init(&mut esp_hal::delay::Delay::new())
        .unwrap();

    display.clear(Rgb565::BLACK).unwrap();
    rprintln!("Display initialized!");

    let config = EmbeddedBackendConfig {
        font_regular: mousefood::fonts::MONO_6X13,
        font_bold: Some(mousefood::fonts::MONO_6X13_BOLD),
        font_italic: Some(mousefood::fonts::MONO_6X13_ITALIC),
        vertical_alignment: mousefood::TerminalAlignment::Center,
        horizontal_alignment: mousefood::TerminalAlignment::Center,
        color_theme: mousefood::ColorTheme::default(),
        flush_callback: alloc::boxed::Box::new(|_| {}),
    };

    let backend = EmbeddedBackend::new(&mut display, config);
    let mut terminal = Terminal::new(backend).unwrap();
    rprintln!("Terminal created!");

    let mut app = App::new();

    rprintln!("Starting UI loop...");

    let mut last_brightness = 16u8;

    loop {
        // 从通道接收编码器事件（带超时，以便定期刷新UI）
        match embassy_time::with_timeout(Duration::from_millis(50), ENCODER_CHANNEL.receive()).await {
            Ok(event) => {
                rprintln!("Received event: {:?}", event);
                
                // 如果处于 BLE 键盘模式，发送键盘事件
                if app.is_ble_mode() {
                    match event {
                        EncoderEvent::Clockwise => {
                            let _ = BLE_KEY_CHANNEL.try_send(BleKeyEvent::Down);
                        }
                        EncoderEvent::CounterClockwise => {
                            let _ = BLE_KEY_CHANNEL.try_send(BleKeyEvent::Up);
                        }
                        _ => {}
                    }
                }
                
                app.handle_event(event);
            }
            Err(_) => {
                // 超时，继续刷新UI
            }
        }

        // 检查亮度是否改变，如果改变则更新硬件
        let current_brightness = app.get_brightness();
        if current_brightness != last_brightness {
			let from = MAX_BRIGHTNESS_STEPS - last_brightness;
        	let to   = MAX_BRIGHTNESS_STEPS - current_brightness;
            rprintln!("Brightness updated to: {}", current_brightness);
            // 模拟 PWM：根据亮度值控制背光
            // 128 为阈值，>= 128 点亮，< 128 关闭
            // if current_brightness >= 128 {
            //     backlight.set_high();
            // } else {
            //     backlight.set_low();
            // }
			let num;
			if to > from {
				num  = to - from;
			} else {
				num = MAX_BRIGHTNESS_STEPS - from + to;
			}
			rprintln!("Adjusting backlight: from {} to {}, steps {}", from, to, num);
			for _ in 0..num {
				backlight.set_low();
				backlight.set_high();
			}
            last_brightness = current_brightness;
        }

        terminal.draw(|frame| app.render(frame)).unwrap();
    }
}