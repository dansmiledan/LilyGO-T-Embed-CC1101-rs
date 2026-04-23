//! SD卡文件系统管理模块
//!
//! 提供全局文件系统访问接口，支持文件浏览、读取、写入等操作。
//! 使用 `embedded-sdmmc` 库通过 SPI 访问 FAT32 格式的 SD 卡。

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use embedded_hal::delay::DelayNs;
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{
    LfnBuffer, Mode, SdCard, ShortFileName, TimeSource, Timestamp, VolumeIdx, VolumeManager,
};

// =============================================================================
// 公共类型
// =============================================================================

/// 目录条目信息
#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    /// 长文件名（用于显示）
    pub name: String,
    /// 短文件名（用于文件系统操作，如 open_dir）
    pub short_name: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件大小（字节）
    pub size: u32,
}

/// SD卡错误类型
#[derive(Debug)]
pub enum SdError {
    /// 未初始化
    NotInitialized,
    /// 无此目录
    NoDirectory,
    /// 文件未找到
    NotFound,
    /// 读取错误
    ReadError,
    /// 写入错误
    WriteError,
    /// SD卡错误
    SdCardError,
    /// 文件名错误
    FilenameError,
    /// 目录过深（句柄不足）
    DirTooDeep,
    /// 文件过大
    FileTooLarge,
}

/// 文件系统操作接口
///
/// 各UI节点和其他模块通过此接口访问SD卡文件系统。
pub trait FileSystem {
    /// 列出指定目录下的所有条目
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntryInfo>, SdError>;

    /// 读取整个文件内容
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, SdError>;

    /// 读取文件内容为字符串
    fn read_file_to_string(&mut self, path: &str) -> Result<String, SdError> {
        let bytes = self.read_file(path)?;
        String::from_utf8(bytes).map_err(|_| SdError::ReadError)
    }

    /// 写入文件（覆盖已有文件）
    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), SdError>;

    /// 检查文件是否存在
    fn file_exists(&mut self, path: &str) -> bool;
}

// =============================================================================
// 全局文件系统实例（单线程安全）
// =============================================================================

static mut GLOBAL_FS: Option<Box<dyn FileSystem>> = None;

/// 初始化全局文件系统
///
/// # Safety
///
/// 应在 `main` 中仅调用一次，且在开始文件操作之前。
/// 本固件为单线程执行模型，无并发访问风险。
pub fn init_fs(fs: Box<dyn FileSystem>) {
    unsafe {
        GLOBAL_FS = Some(fs);
    }
}

fn with_fs<F, R>(f: F) -> Result<R, SdError>
where
    F: FnOnce(&mut dyn FileSystem) -> Result<R, SdError>,
{
    unsafe {
        let ptr = core::ptr::addr_of_mut!(GLOBAL_FS);
        match (*ptr).as_mut() {
            Some(fs) => f(fs.as_mut()),
            None => Err(SdError::NotInitialized),
        }
    }
}

/// 列出指定目录下的文件和子目录
pub fn list_directory(path: &str) -> Result<Vec<DirEntryInfo>, SdError> {
    with_fs(|fs| fs.list_dir(path))
}

/// 读取整个文件内容
pub fn read_file(path: &str) -> Result<Vec<u8>, SdError> {
    with_fs(|fs| fs.read_file(path))
}

/// 读取文件内容为字符串
pub fn read_file_to_string(path: &str) -> Result<String, SdError> {
    with_fs(|fs| fs.read_file_to_string(path))
}

/// 写入文件（覆盖已有文件）
pub fn write_file(path: &str, data: &[u8]) -> Result<(), SdError> {
    with_fs(|fs| fs.write_file(path, data))
}

/// 检查文件是否存在
pub fn file_exists(path: &str) -> bool {
    with_fs(|fs| Ok(fs.file_exists(path))).unwrap_or(false)
}

// =============================================================================
// SD卡管理器实现
// =============================================================================

/// 时间源（固定时间，无需RTC）
pub struct DummyTimeSource;

impl TimeSource for DummyTimeSource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}

/// SD卡文件系统具体实现
///
/// 封装 `embedded-sdmmc` 的 `VolumeManager`，提供高层文件操作。
pub struct SdManagerImpl<SPI, DELAYER>
where
    SPI: SpiDevice<u8>,
    DELAYER: DelayNs,
{
    volume_mgr: VolumeManager<SdCard<SPI, DELAYER>, DummyTimeSource>,
    volume: embedded_sdmmc::RawVolume,
}

impl<SPI, DELAYER> SdManagerImpl<SPI, DELAYER>
where
    SPI: SpiDevice<u8>,
    DELAYER: DelayNs,
{
    /// 创建并初始化SD卡管理器
    pub fn new(spi: SPI, delayer: DELAYER) -> Result<Self, SdError> {
        let sdcard = SdCard::new(spi, delayer);
        let mut volume_mgr = VolumeManager::new(sdcard, DummyTimeSource);
        let volume = volume_mgr
            .open_volume(VolumeIdx(0))
            .map_err(|_| SdError::SdCardError)?;
        let raw_volume = volume.to_raw_volume();

        Ok(Self {
            volume_mgr,
            volume: raw_volume,
        })
    }

    /// 导航到指定路径的目录
    ///
    /// 返回打开的目录句柄，调用者负责关闭。
    fn navigate_to_dir(&mut self, path: &str) -> Result<embedded_sdmmc::RawDirectory, SdError> {
        let mut dir = self
            .volume_mgr
            .open_root_dir(self.volume)
            .map_err(|_| SdError::NoDirectory)?;

        for component in path.split('/').filter(|s| !s.is_empty()) {
            let new_dir = self
                .volume_mgr
                .open_dir(dir, component)
                .map_err(|_| SdError::NotFound)?;
            let _ = self.volume_mgr.close_dir(dir);
            dir = new_dir;
        }
        Ok(dir)
    }
}

impl<SPI, DELAYER> FileSystem for SdManagerImpl<SPI, DELAYER>
where
    SPI: SpiDevice<u8>,
    DELAYER: DelayNs,
{
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntryInfo>, SdError> {
        let dir = self.navigate_to_dir(path)?;
        let mut entries = Vec::new();
        let mut lfn_storage = [0u8; 256];
        let mut lfn_buffer = LfnBuffer::new(&mut lfn_storage);

        self.volume_mgr
            .iterate_dir_lfn(dir, &mut lfn_buffer, |entry, lfn| {
                let short_name = short_name_to_string(&entry.name);
                let name = lfn
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| short_name.clone());
                if short_name != "." && short_name != ".." {
                    entries.push(DirEntryInfo {
                        name,
                        short_name,
                        is_dir: entry.attributes.is_directory(),
                        size: entry.size,
                    });
                }
            })
            .map_err(|_| SdError::ReadError)?;

        self.volume_mgr.close_dir(dir).ok();
        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, SdError> {
        let (dir_path, file_name) = split_path(path);
        let dir = self.navigate_to_dir(&dir_path)?;

        let file = self
            .volume_mgr
            .open_file_in_dir(dir, file_name.as_str(), Mode::ReadOnly)
            .map_err(|_| SdError::NotFound)?;

        let mut data = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let n = self
                .volume_mgr
                .read(file, &mut buffer)
                .map_err(|_| SdError::ReadError)?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..n]);
        }

        self.volume_mgr.close_file(file).ok();
        self.volume_mgr.close_dir(dir).ok();
        Ok(data)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), SdError> {
        let (dir_path, file_name) = split_path(path);
        let dir = self.navigate_to_dir(&dir_path)?;

        let file = self
            .volume_mgr
            .open_file_in_dir(dir, file_name.as_str(), Mode::ReadWriteCreateOrTruncate)
            .map_err(|_| SdError::WriteError)?;

        self.volume_mgr
            .write(file, data)
            .map_err(|_| SdError::WriteError)?;

        self.volume_mgr.close_file(file).ok();
        self.volume_mgr.close_dir(dir).ok();
        Ok(())
    }

    fn file_exists(&mut self, path: &str) -> bool {
        let (dir_path, file_name) = split_path(path);
        let Ok(dir) = self.navigate_to_dir(&dir_path) else {
            return false;
        };
        let exists = self
            .volume_mgr
            .find_directory_entry(dir, file_name.as_str())
            .map(|entry| !entry.attributes.is_directory())
            .unwrap_or(false);
        self.volume_mgr.close_dir(dir).ok();
        exists
    }
}

// =============================================================================
// 辅助函数
// =============================================================================

/// 将路径拆分为目录部分和文件名部分
pub fn split_path(path: &str) -> (String, String) {
    if let Some(pos) = path.rfind('/') {
        let dir = path[..pos].to_string();
        let file = path[pos + 1..].to_string();
        (dir, file)
    } else {
        (String::new(), path.to_string())
    }
}

/// 将 `embedded-sdmmc` 的短文件名转换为可读字符串
pub fn short_name_to_string(name: &ShortFileName) -> String {
    let base = core::str::from_utf8(name.base_name()).unwrap_or("???");
    let ext = core::str::from_utf8(name.extension()).unwrap_or("");
    if ext.is_empty() {
        base.to_string()
    } else {
        format!("{}.{}", base, ext)
    }
}

/// 格式化文件大小为人类可读字符串
pub fn format_size(size: u32) -> String {
    if size < 1024 {
        format!("{}B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1}K", size as f32 / 1024.0)
    } else {
        format!("{:.1}M", size as f32 / (1024.0 * 1024.0))
    }
}
