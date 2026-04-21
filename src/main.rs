#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod input;
mod app;
mod backlight;
mod ble_hid;

use embassy_executor::Spawner;
use embassy_time::Duration;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig, Pin};
use esp_hal::ledc::{Ledc, LSGlobalClkSource, LowSpeed};
use esp_hal::ledc::timer::{self, TimerIFace};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::psram::{ PsramConfig, PsramSize };
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::Terminal;
use log::info;

use input::{init_encoder, encoder_task, ENCODER_CHANNEL};
use app::{App, Command};
use backlight::Backlight;
use ble_hid::{BLE_KEY_CHANNEL, BLE_CONTROL_CHANNEL};

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
	log::error!("Panic occurred: {:?}", p);
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

/// 执行 App 产生的硬件命令
fn execute_command(cmd: Command, backlight: &Backlight<'_>) {
    match cmd {
        Command::Backlight(level) => {
            info!("Brightness updated to: {}", level);
            backlight.set_brightness(level);
        }
        Command::BleControl(event) => {
            match event {
                ble_hid::BleControlEvent::Start => info!("进入 BLE 键盘模式，启动蓝牙广播"),
                ble_hid::BleControlEvent::Stop => info!("退出 BLE 键盘模式，停止蓝牙广播"),
            }
            let _ = BLE_CONTROL_CHANNEL.try_send(event);
        }
        Command::BleKey(event) => {
            let _ = BLE_KEY_CHANNEL.try_send(event);
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let psram = PsramConfig {
        size: PsramSize::Size(8 * 1024 * 1024),
        ..PsramConfig::default()
    };
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max()).with_psram(psram);
    let peripherals = esp_hal::init(config);

    // 初始化堆分配器
    esp_alloc::heap_allocator!(size: 128 * 1024);

    let (psram_start, psram_size) = esp_hal::psram::psram_raw_parts(&peripherals.PSRAM);
    info!("PSRAM 信息: 起始地址={:p}, 大小={} 字节", psram_start, psram_size);

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

    // 启动系统定时器
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    info!("T-Embed CC1101 UI Starting...");

    // ========== 硬件初始化 ==========

    // 编码器输入
    let encoder = init_encoder(
        peripherals.GPIO4.degrade(),
        peripherals.GPIO5.degrade(),
        peripherals.GPIO0.degrade(),
        peripherals.GPIO6.degrade(),
    );
    spawner.spawn(encoder_task(encoder).unwrap());

    // BLE 键盘任务（初始状态等待启动命令，不广播）
    let bluetooth = peripherals.BT;
    spawner.spawn(ble_hid::run_ble_keyboard(bluetooth).unwrap());

    // SPI 显示屏
    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(SpiMode::_0);

    let spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO11)
        .with_mosi(peripherals.GPIO9);

    let cs = Output::new(peripherals.GPIO41, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());

    // 背光（LEDC 硬件 PWM）
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0.configure(timer::config::Config {
        duty: timer::config::Duty::Duty5Bit,
        clock_source: timer::LSClockSource::APBClk,
        frequency: Rate::from_khz(20),
    }).unwrap();

    let backlight = Backlight::new(&ledc, &lstimer0, peripherals.GPIO21);

    // 初始化 ST7789 显示屏
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

    // ========== 应用主循环 ==========

    let mut app = App::new();
    info!("Starting UI loop...");

    loop {
        // 接收编码器事件（带超时，以便定期刷新UI）
        match embassy_time::with_timeout(Duration::from_millis(50), ENCODER_CHANNEL.receive()).await {
            Ok(event) => {
                info!("Received event: {:?}", event);
                let commands = app.handle_event(event);
                for cmd in commands {
                    execute_command(cmd, &backlight);
                }
            }
            Err(_) => {
                // 超时，继续刷新UI
            }
        }

        // 定期更新（检测亮度变化等）
        let commands = app.tick();
        for cmd in commands {
            execute_command(cmd, &backlight);
        }

        terminal.draw(|frame| app.render(frame)).unwrap();
    }
}
