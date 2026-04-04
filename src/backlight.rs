use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

// 全局通道用于传递亮度更新请求（预留接口）
#[allow(dead_code)]
pub static BRIGHTNESS_CHANNEL: Channel<CriticalSectionRawMutex, u8, 4> = Channel::new();
