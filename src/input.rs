use embassy_futures::select::{select4, Either4};
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
    ConfirmPressed,
    ConfirmReleased,
    BackPressed,
    BackReleased,
}

struct EncoderState {
    position: i32,
    position_ext: i32,
    position_ext_prev: i32,
    old_state: u8,
}

struct ConfirmButtonState {
    pressed: bool,
}

struct BackButtonState {
    pressed: bool,
}

pub struct Encoder<'a> {
    pin_a: Input<'a>,
    pin_b: Input<'a>,
    confirm_button: Input<'a>,
    back_button: Input<'a>,
}

pub fn init_encoder<'a>(
    pin_a: esp_hal::gpio::AnyPin<'a>,
    pin_b: esp_hal::gpio::AnyPin<'a>,
    confirm_button: esp_hal::gpio::AnyPin<'a>,
    back_button: esp_hal::gpio::AnyPin<'a>,
) -> Encoder<'a> {
    let config = InputConfig::default().with_pull(Pull::Up);

    let mut pin_a = Input::new(pin_a, config.clone());
    let mut pin_b = Input::new(pin_b, config.clone());
    let mut confirm_button = Input::new(confirm_button, config.clone());
    let mut back_button = Input::new(back_button, config);

    pin_a.listen(Event::AnyEdge);
    pin_b.listen(Event::AnyEdge);
    confirm_button.listen(Event::AnyEdge);
    back_button.listen(Event::AnyEdge);

    Encoder {
        pin_a,
        pin_b,
        confirm_button,
        back_button,
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

    let mut confirm_state = ConfirmButtonState {
        pressed: false,
    };

    let mut back_state = BackButtonState {
        pressed: false,
    };

    rprintln!("Encoder task started");

    loop {
        // 等待编码器和两个按钮的边沿触发
        match select4(
            encoder.pin_a.wait_for_any_edge(),
            encoder.pin_b.wait_for_any_edge(),
            encoder.confirm_button.wait_for_any_edge(),
            encoder.back_button.wait_for_any_edge(),
        )
        .await
        {
            Either4::First(_) | Either4::Second(_) => {
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
            Either4::Third(_) => {
                // 确认键被按下
                if !confirm_state.pressed {
                    confirm_state.pressed = true;
                    rprintln!("Confirm button pressed");
                    ENCODER_CHANNEL.send(EncoderEvent::ConfirmPressed).await;
                }
            }
            Either4::Fourth(_) => {
                // 返回键被按下
                if !back_state.pressed {
                    back_state.pressed = true;
                    rprintln!("Back button pressed");
                    ENCODER_CHANNEL.send(EncoderEvent::BackPressed).await;
                }
            }
        }

        // 检查确认键释放
        if confirm_state.pressed && encoder.confirm_button.is_high() {
            confirm_state.pressed = false;
            rprintln!("Confirm button released");
            ENCODER_CHANNEL.send(EncoderEvent::ConfirmReleased).await;
        }

        // 检查返回键释放
        if back_state.pressed && encoder.back_button.is_high() {
            back_state.pressed = false;
            rprintln!("Back button released");
            ENCODER_CHANNEL.send(EncoderEvent::BackReleased).await;
        }
    }
}
