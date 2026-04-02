#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_time::Duration;

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Event, Input, InputConfig, Level, Output, OutputConfig, Pull, Pin};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::Terminal;
use rtt_target::rprintln;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    rtt_target::rtt_init_print!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 128 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    rprintln!("T-Embed CC1101 UI Starting...");

    let mut encoder_state = EncoderState {
        position: 0,
        position_ext: 0,
        position_ext_prev: 0,
        old_state: 0,
    };

    let mut button_state = ButtonState {
        pressed: false,
        press_time: 0,
    };

    let mut encoder = init_encoder(
        peripherals.GPIO4.degrade(),
        peripherals.GPIO5.degrade(),
        peripherals.GPIO0.degrade(),
    );

    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(SpiMode::_0);

    let spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO11)
        .with_mosi(peripherals.GPIO9);

    let cs = Output::new(peripherals.GPIO41, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default());
    let _backlight = Output::new(peripherals.GPIO21, Level::High, OutputConfig::default());

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

    loop {
        let event = update_encoder(&mut encoder, &mut encoder_state, &mut button_state);

        if event == EncoderEvent::Clockwise || event == EncoderEvent::CounterClockwise {
            app.handle_event(event);
        }

        terminal.draw(|frame| app.render(frame)).unwrap();

        Timer::after(Duration::from_millis(50)).await;
    }
}

use embassy_time::Instant;

const KNOBDIR: [i8; 16] = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0];

struct EncoderState {
    position: i32,
    position_ext: i32,
    position_ext_prev: i32,
    old_state: u8,
}

struct ButtonState {
    pressed: bool,
    press_time: u64,
}

struct Encoder<'a> {
    pin_a: Input<'a>,
    pin_b: Input<'a>,
    pin_button: Input<'a>,
}

fn init_encoder<'a>(
    pin_a: esp_hal::gpio::AnyPin<'a>,
    pin_b: esp_hal::gpio::AnyPin<'a>,
    pin_button: esp_hal::gpio::AnyPin<'a>,
) -> Encoder<'a> {
    let config = InputConfig::default().with_pull(Pull::Up);

    let mut pin_a = Input::new(pin_a, config.clone());
    let mut pin_b = Input::new(pin_b, config.clone());
    let mut pin_button = Input::new(pin_button, config);

    pin_a.listen(Event::AnyEdge);
    pin_b.listen(Event::AnyEdge);
    pin_button.listen(Event::FallingEdge);

    Encoder {
        pin_a,
        pin_b,
        pin_button,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncoderEvent {
    None,
    Clockwise,
    CounterClockwise,
    ButtonPressed,
    ButtonReleased,
    LongPress,
}

fn update_encoder(
    encoder: &mut Encoder,
    state: &mut EncoderState,
    button_state: &mut ButtonState,
) -> EncoderEvent {
    let sig1 = if encoder.pin_a.is_high() { 1u8 } else { 0u8 };
    let sig2 = if encoder.pin_b.is_high() { 1u8 } else { 0u8 };

    let this_state = sig1 | (sig2 << 1);
	rprintln!("Encoder state: sig1={}, sig2={}, this_state={}", sig1, sig2, this_state);
    // 只有状态变化时才更新
    if state.old_state != this_state {
        let index = this_state | (state.old_state << 2);
        let delta = KNOBDIR[index as usize];

        state.position += delta as i32;
        state.old_state = this_state;

        // LatchMode::FOUR0: 在状态0时更新外部位置并检测旋转事件
        if this_state == 3 {
            let _prev_ext = state.position_ext;
            state.position_ext = state.position >> 2;
            
            // 检测旋转方向
            if state.position_ext > state.position_ext_prev {
                state.position_ext_prev = state.position_ext;
                return EncoderEvent::Clockwise;
            } else if state.position_ext < state.position_ext_prev {
                state.position_ext_prev = state.position_ext;
                return EncoderEvent::CounterClockwise;
            }
        }
    }

    // 按钮事件处理
    if encoder.pin_button.is_low() {
        if !button_state.pressed {
            button_state.pressed = true;
            button_state.press_time = Instant::now().elapsed().as_micros() as u64;
            return EncoderEvent::ButtonPressed;
        } else {
            let elapsed = (Instant::now().elapsed().as_micros() as u64)
                .saturating_sub(button_state.press_time);
            if elapsed >= 2_000_000 {
                button_state.press_time = 0;
                return EncoderEvent::LongPress;
            }
        }
    } else if button_state.pressed {
        button_state.pressed = false;
        return EncoderEvent::ButtonReleased;
    }

    EncoderEvent::None
}

struct App {
    selected: usize,
    menu_items: &'static [&'static str],
}

impl App {
    fn new() -> Self {
        Self {
            selected: 0,
            menu_items: &["Wi-Fi", "Bluetooth", "RFID/NFC", "Sub-GHz", "IR Remote", "GPS", "Settings"],
        }
    }

    fn handle_event(&mut self, event: EncoderEvent) {
        match event {
            EncoderEvent::Clockwise => {
                if self.selected < self.menu_items.len() - 1 {
                    self.selected += 1;
                }
            }
            EncoderEvent::CounterClockwise => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            _ => {}
        }
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        use ratatui::layout::Alignment;
        use ratatui::prelude::Widget;
        use ratatui::style::{Color, Style};
        use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};

        let chunks = ratatui::layout::Layout::default()
            .constraints([
                ratatui::layout::Constraint::Length(3),
                ratatui::layout::Constraint::Min(0),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(frame.area());

        let title = Paragraph::new("T-Embed CC1101")
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .alignment(Alignment::Center);

        title.render(chunks[0], frame.buffer_mut());

        let items: alloc::vec::Vec<ListItem> = self
            .menu_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.selected {
                    Style::default().fg(Color::Black).bg(Color::White).bold()
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(*item).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .title("Menu"),
            )
            .style(Style::default().bg(Color::Black))
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("> ");

        list.render(chunks[1], frame.buffer_mut());

        let hint = Paragraph::new("Rotate: Navigate | Press: Select | LongPress: Back")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        hint.render(chunks[2], frame.buffer_mut());
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
