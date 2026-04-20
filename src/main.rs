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
use esp_hal::gpio::{Level, Output, OutputConfig, Pin, DriveMode};
use esp_hal::ledc::{Ledc, LSGlobalClkSource, LowSpeed};
use esp_hal::ledc::timer::{self, TimerIFace};
use esp_hal::ledc::channel::{self, ChannelIFace};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::psram::{ PsramConfig, PsramSize };
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::Terminal;
use log::info;

// use rtt_target::rtt_init_print;

use input::{init_encoder, encoder_task, ENCODER_CHANNEL, EncoderEvent};
use ui::App;
use ble_hid::{BleKeyEvent, BLE_KEY_CHANNEL};

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
	log::error!("Panic occurred: {:?}", p);
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // rtt_init_print!();
    // let _ = log::set_logger(&RttLogger);
    // log::set_max_level(log::LevelFilter::Debug);
    esp_println::logger::init_logger_from_env();
    let psram = PsramConfig {
        size: PsramSize::Size(8 * 1024 * 1024),
        ..PsramConfig::default() 
    };
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max()).with_psram(psram);
    let peripherals = esp_hal::init(config);

    // 初始化堆分配器
    esp_alloc::heap_allocator!(size: 128 * 1024);
    
    // esp-hal 1.0 自动初始化 PSRAM（如果启用了 psram feature）
    // 使用 psram_raw_parts 获取 PSRAM 信息
    let (psram_start, psram_size) = esp_hal::psram::psram_raw_parts(&peripherals.PSRAM);
    info!("PSRAM 信息: 起始地址={:p}, 大小={} 字节", psram_start, psram_size);
    
    // 如果 PSRAM 已初始化，将其添加到堆
    if psram_size > 0 {
        unsafe {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                psram_start,
                psram_size,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }
        info!("PSRAM 已添加到堆分配器");
    }

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    info!("T-Embed CC1101 UI Starting...");

    // 创建编码器实例
    let encoder = init_encoder(
        peripherals.GPIO4.degrade(),
        peripherals.GPIO5.degrade(),
        peripherals.GPIO0.degrade(),
        peripherals.GPIO6.degrade(),
    );

    // 启动编码器任务
    spawner.spawn(encoder_task(encoder).unwrap());

    // 启动 BLE 键盘监听任务
    let bluetooth = peripherals.BT;
    spawner.spawn(ble_hid::run_ble_keyboard(bluetooth).unwrap());


    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(SpiMode::_0);

    let spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO11)
        .with_mosi(peripherals.GPIO9);

    let cs = Output::new(peripherals.GPIO41, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());

    // 初始化 LEDC 硬件 PWM 控制 AW9364 背光
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0.configure(timer::config::Config {
        duty: timer::config::Duty::Duty5Bit,
        clock_source: timer::LSClockSource::APBClk,
        frequency: Rate::from_khz(20),
    }).unwrap();

    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO21);
    channel0.configure(channel::config::Config {
        timer: &lstimer0,
        duty_pct: 100,
        drive_mode: DriveMode::PushPull,
    }).unwrap();

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
    info!("Display initialized!");

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
    info!("Terminal created!");

    let mut app = App::new();

    info!("Starting UI loop...");

    let mut last_brightness = 16u8;

    loop {
        // 从通道接收编码器事件（带超时，以便定期刷新UI）
        match embassy_time::with_timeout(Duration::from_millis(50), ENCODER_CHANNEL.receive()).await {
            Ok(event) => {
                info!("Received event: {:?}", event);
                
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
            let duty_pct = ((current_brightness as u16 * 100) / 16).min(100) as u8;
            info!("Brightness updated to: {}, duty {}%", current_brightness, duty_pct);
            channel0.set_duty(duty_pct).unwrap();
            last_brightness = current_brightness;
        }

        terminal.draw(|frame| app.render(frame)).unwrap();
    }
}

// BLE 键盘监听任务 - 简化版本，用于接收和处理键盘事件
// #[embassy_executor::task]
// async fn ble_keyboard_listener() {
//     rprintln!("🎧 BLE 键盘监听任务已启动");
//     let mut counter = 0u32;
    
//     loop {
//         // 监听 BLE_KEY_CHANNEL 中的事件
//         match embassy_time::with_timeout(
//             embassy_time::Duration::from_millis(1000),
//             BLE_KEY_CHANNEL.receive()
//         ).await {
//             Ok(event) => {
//                 counter += 1;
//                 let key_name = match event {
//                     BleKeyEvent::Up => "↑ UP (0x52)",
//                     BleKeyEvent::Down => "↓ DOWN (0x51)",
//                 };
//                 rprintln!("[{}] 🔑 BLE 键盘事件: {}", counter, key_name);
//             }
//             Err(_) => {
//                 // 超时，继续等待
//             }
//         }
//     }
// }