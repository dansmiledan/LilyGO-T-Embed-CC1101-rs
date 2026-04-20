use esp_hal::gpio::DriveMode;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::ledc::{Ledc, LowSpeed};
use esp_hal::ledc::channel::{self, ChannelIFace};
use esp_hal::ledc::timer;

/// 背光驱动，基于 LEDC 硬件 PWM 控制 AW9364
pub struct Backlight<'a> {
    channel: channel::Channel<'a, LowSpeed>,
}

impl<'a> Backlight<'a> {
    /// 创建背光驱动实例
    ///
    /// # Arguments
    /// * `ledc` - LEDC 驱动引用
    /// * `timer` - 已配置的低速定时器引用
    /// * `gpio21` - 背光控制引脚（GPIO21）
    pub fn new(
        ledc: &'a Ledc<'a>,
        timer: &'a timer::Timer<'a, LowSpeed>,
        gpio21: impl PeripheralOutput<'a>,
    ) -> Self {
        let mut channel0 = ledc.channel(channel::Number::Channel0, gpio21);
        channel0
            .configure(channel::config::Config {
                timer,
                duty_pct: 100,
                drive_mode: DriveMode::PushPull,
            })
            .unwrap();

        Self { channel: channel0 }
    }

    /// 设置背光亮度（0-100，100 为最亮）
    pub fn set_brightness(&self, level: u8) {
        let duty = level.min(100);
        self.channel.set_duty(duty).unwrap();
    }
}
