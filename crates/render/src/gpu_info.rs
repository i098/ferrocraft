//! The GPU lines of F3: `GL_VENDOR`, `GL_RENDERER` and `GL_VERSION` from the wgpu adapter.

/// Strings for `Display: WxH (vendor)`, the renderer line and the version line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuInfo {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
}

impl GpuInfo {
    pub fn from_adapter(i: &wgpu::AdapterInfo) -> GpuInfo {
        let vendor = match i.vendor {
            0x10DE => "NVIDIA Corporation",
            0x1002 => "ATI Technologies Inc.",
            0x8086 => "Intel",
            0x106B => "Apple",
            0x5143 => "Qualcomm",
            0x10005 => "Mesa",
            _ => "Unknown",
        };
        let version = if i.driver_info.is_empty() {
            format!("{:?} {}", i.backend, i.driver)
        } else {
            format!("{:?} {} {}", i.backend, i.driver, i.driver_info)
        };
        GpuInfo {
            vendor: vendor.to_owned(),
            renderer: i.name.clone(),
            version: version.trim().to_owned(),
        }
    }
}
