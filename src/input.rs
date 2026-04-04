use embassy_futures::select::{select, select3, Either, Either3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use esp_hal::gpio::{Event, Input, InputConfig, Pull};
use rtt_target::rprintln;

// 全局通道用于传递编码器事件
pub static ENCODER_CHANNEL: Channel<CriticalSectionRawMutex, EncoderEvent, 8> = Channel::new();

const KNOBDIR: [i8; 16] = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderEvent {
    Clockwise,
    CounterClockwise,
    ButtonPressed,
    ButtonReleased,
    Button2Pressed,
    Button2Released,
}

struct EncoderState {
    position: i32,
    position_ext: i32,
    position_ext_prev: i32,
    old_state: u8,
}

struct ButtonState {
    pressed: bool,
}

struct Button2State {
    pressed: bool,
}

pub struct Encoder<'a> {
    pin_a: Input<'a>,
    pin_b: Input<'a>,
    pin_button: Input<'a>,
    pin_button2: Input<'a>,
}

pub fn init_encoder<'a>(
    pin_a: esp_hal::gpio::AnyPin<'a>,
    pin_b: esp_hal::gpio::AnyPin<'a>,
    pin_button: esp_hal::gpio::AnyPin<'a>,
    pin_button2: esp_hal::gpio::AnyPin<'a>,
) -> Encoder<'a> {
    let config = InputConfig::default().with_pull(Pull::Up);

    let mut pin_a = Input::new(pin_a, config.clone());
    let mut pin_b = Input::new(pin_b, config.clone());
    let mut pin_button = Input::new(pin_button, config.clone());
    let mut pin_button2 = Input::new(pin_button2, config);

    pin_a.listen(Event::AnyEdge);
    pin_b.listen(Event::AnyEdge);
    pin_button.listen(Event::FallingEdge);
    pin_button2.listen(Event::FallingEdge);

    Encoder {
        pin_a,
        pin_b,
        pin_button,
        pin_button2,
    }
}

#[embassy_executor::task]
pub async fn encoder_task(mut encoder: Encoder<'static>) {
    let mut state = EncoderState {
        position: 0,
        position_ext: 0,
        position_ext_prev: 0,
        old_state: 0,
    };

    let mut button_state = ButtonState {
        pressed: false,
    };

    let mut button2_state = Button2State {
        pressed: false,
    };

    rprintln!("Encoder task started");

    loop {
        // 等待编码器和两个按钮的边沿触发
        let encoder_result = select3(
            encoder.pin_a.wait_for_any_edge(),
            encoder.pin_b.wait_for_any_edge(),
            encoder.pin_button.wait_for_falling_edge(),
        );

        let button2_result = encoder.pin_button2.wait_for_falling_edge();

        match select(encoder_result, button2_result).await {
            Either::First(enc_res) => {
                match enc_res {
                    Either3::First(_) | Either3::Second(_) => {
                        // A 或 B 引脚有变化，处理旋转
                        let sig1 = if encoder.pin_a.is_high() { 1u8 } else { 0u8 };
                        let sig2 = if encoder.pin_b.is_high() { 1u8 } else { 0u8 };

                        let this_state = sig1 | (sig2 << 1);

                        // 只有状态变化时才更新
                        if state.old_state != this_state {
                            let index = this_state | (state.old_state << 2);
                            let delta = KNOBDIR[index as usize];

                            state.position += delta as i32;
                            state.old_state = this_state;

                            // LatchMode::FOUR0: 在状态0时更新外部位置并检测旋转事件
                            if this_state == 3 {
                                state.position_ext = state.position >> 2;

                                // 检测旋转方向
                                if state.position_ext > state.position_ext_prev {
                                    state.position_ext_prev = state.position_ext;
                                    rprintln!("Clockwise");
                                    ENCODER_CHANNEL.send(EncoderEvent::Clockwise).await;
                                } else if state.position_ext < state.position_ext_prev {
                                    state.position_ext_prev = state.position_ext;
                                    rprintln!("CounterClockwise");
                                    ENCODER_CHANNEL.send(EncoderEvent::CounterClockwise).await;
                                }
                            }
                        }
                    }
                    Either3::Third(_) => {
                        // 按钮被按下
                        if !button_state.pressed {
                            button_state.pressed = true;
                            rprintln!("Button pressed");
                            ENCODER_CHANNEL.send(EncoderEvent::ButtonPressed).await;
                        }
                    }
                }
            }
            Either::Second(_) => {
                // 第二个按钮被按下
                if !button2_state.pressed {
                    button2_state.pressed = true;
                    rprintln!("Button2 pressed");
                    ENCODER_CHANNEL.send(EncoderEvent::Button2Pressed).await;
                }
            }
        }

        // 检查第一个按钮释放
        if button_state.pressed && encoder.pin_button.is_high() {
            button_state.pressed = false;
            rprintln!("Button released");
            ENCODER_CHANNEL.send(EncoderEvent::ButtonReleased).await;
        }

        // 检查第二个按钮释放
        if button2_state.pressed && encoder.pin_button2.is_high() {
            button2_state.pressed = false;
            rprintln!("Button2 released");
            ENCODER_CHANNEL.send(EncoderEvent::Button2Released).await;
        }
    }
}
