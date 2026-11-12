//! 终端设备信息采集（对齐 geo-pet `geopet_os::device::snapshot`）。
//!
//! 随 channel 握手 `Hello.device_info` 上报服务端登记。尽力而为采集：
//! 单个字段失败不影响其余字段，返回默认值（空串/false）。
//! 含少量系统命令调用，同步阻塞——调用方应避免在 async 上下文直接调用
//! （经 `spawn_blocking` 执行）。
//!
//! 注意：禁止直连 IOBluetooth 等隐私框架 API——打包后的 .app 缺少
//! NSBluetoothAlwaysUsageDescription，macOS TCC 会直接终止进程（SIGABRT，
//! v0.1.42 发版即因此必崩）。

/// 设备信息快照（连接握手时上报）。
#[derive(Debug, Clone, Default)]
pub struct DeviceSnapshot {
    /// 品牌厂商（如 Apple）
    pub device_brand: String,
    /// 机型型号（如 Mac16,8）
    pub device_model: String,
    /// 设备形态（laptop/desktop）
    pub device_type: String,
    /// 主屏宽（物理像素）
    pub screen_width: i32,
    /// 主屏高（物理像素）
    pub screen_height: i32,
    /// 操作系统（如 macos）
    pub os_name: String,
    /// 系统版本（如 27.0.1）
    pub os_version: String,
    /// Wi-Fi 开关状态
    pub wifi_enabled: bool,
    /// 蓝牙开关状态（不再采集，恒为 false；直连 IOBluetooth 会触发 macOS TCC 终止）
    pub bluetooth_enabled: bool,
}

/// 采集设备信息快照（同步阻塞，调用方应避免在 async 上下文直接调用）。
pub fn snapshot() -> DeviceSnapshot {
    imp::snapshot()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::DeviceSnapshot;

    // 主显示器物理像素尺寸（Tauri primary_monitor 的网关侧等价实现，
    // 桌面/Web 两种运行模式均可用）
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(display: u32) -> usize;
        fn CGDisplayPixelsHigh(display: u32) -> usize;
    }

    pub fn snapshot() -> DeviceSnapshot {
        let model = sysctl("hw.model").unwrap_or_default();
        let device_type = if model.starts_with("MacBook") {
            "laptop"
        } else {
            "desktop"
        };
        DeviceSnapshot {
            device_brand: "Apple".into(),
            device_model: model,
            device_type: device_type.into(),
            screen_width: primary_screen().0,
            screen_height: primary_screen().1,
            os_name: std::env::consts::OS.into(),
            os_version: sysctl("kern.osproductversion").unwrap_or_default(),
            wifi_enabled: wifi_enabled(),
            // 不采集：IOBluetooth API 触发 TCC 隐私终止（见模块注释）
            bluetooth_enabled: false,
        }
    }

    /// 主屏物理像素（CoreGraphics；display id 无效时尺寸函数返回 0）。
    fn primary_screen() -> (i32, i32) {
        // SAFETY: 无参 C 查询函数；CoreGraphics 框架由上方链接声明引入
        unsafe {
            let display = CGMainDisplayID();
            (
                CGDisplayPixelsWide(display) as i32,
                CGDisplayPixelsHigh(display) as i32,
            )
        }
    }

    /// sysctl 单键读取（`sysctl -n <key>`）。
    fn sysctl(key: &str) -> Option<String> {
        let out = std::process::Command::new("sysctl").arg("-n").arg(key).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    /// Wi-Fi 电源状态：先定位 Wi-Fi 网卡（`networksetup -listallhardwareports`），再查其电源状态。
    fn wifi_enabled() -> bool {
        let Ok(ports) = std::process::Command::new("networksetup")
            .arg("-listallhardwareports")
            .output()
        else {
            return false;
        };
        let text = String::from_utf8_lossy(&ports.stdout);
        // 形如 "Hardware Port: Wi-Fi\nDevice: en0\n..."（端口名跨语言不翻译），取该段设备名
        let device = text
            .split("\n\n")
            .find(|blk| blk.contains("Wi-Fi"))
            .and_then(|blk| blk.lines().find_map(|l| l.strip_prefix("Device: ")))
            .map_or("en0", str::trim);
        let Ok(out) = std::process::Command::new("networksetup")
            .args(["-getairportpower", device])
            .output()
        else {
            return false;
        };
        String::from_utf8_lossy(&out.stdout).contains(": On")
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::DeviceSnapshot;

    pub fn snapshot() -> DeviceSnapshot {
        DeviceSnapshot {
            os_name: std::env::consts::OS.into(),
            ..Default::default()
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn snapshot_fields_are_populated() {
        let snap = snapshot();
        assert_eq!(snap.device_brand, "Apple");
        assert!(snap.device_model.starts_with("Mac"));
        assert!(["laptop", "desktop"].contains(&snap.device_type.as_str()));
        assert_eq!(snap.os_name, "macos");
        assert!(!snap.os_version.is_empty());
        assert!(snap.screen_width > 0, "主屏宽度应可采集");
        assert!(snap.screen_height > 0, "主屏高度应可采集");
    }
}
