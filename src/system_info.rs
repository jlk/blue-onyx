use tracing::info;

pub fn system_info() -> anyhow::Result<()> {
    info!("System Information:");
    cpu_info()?;
    gpu_info(true)?;
    Ok(())
}

pub fn cpu_model() -> String {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();
    match cpuid.get_processor_brand_string() {
        Some(cpu_brand) => cpu_brand.as_str().to_owned(),
        None => "Unknown".to_owned(),
    }
}

pub fn gpu_model(index: usize) -> String {
    let gpu_names = gpu_info(false).unwrap_or_default();
    gpu_names
        .get(index)
        .cloned()
        .unwrap_or_else(|| "Unknown".to_owned())
}

pub fn cpu_info() -> anyhow::Result<()> {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();

    let cpu_vendor_info = match cpuid.get_vendor_info() {
        Some(vendor_info) => vendor_info.as_str().to_owned(),
        None => "Unknown".to_owned(),
    };

    let cpu_brand = match cpuid.get_processor_brand_string() {
        Some(cpu_brand) => cpu_brand.as_str().to_owned(),
        None => "Unknown".to_owned(),
    };

    info!(
        "CPU | {} | {} | {} Cores | {} Logical Cores",
        cpu_vendor_info,
        cpu_brand,
        num_cpus::get_physical(),
        num_cpus::get()
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn gpu_info(log_info: bool) -> anyhow::Result<Vec<String>> {
    use std::process::Command;

    // Try to get GPU info from nvidia-smi
    let output = match Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            if log_info {
                info!("No NVIDIA GPUs detected (nvidia-smi not available or no GPUs found)");
            }
            return Ok(vec![]);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpu_names: Vec<String> = stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    gpu_names.sort();

    if log_info {
        for device_name in &gpu_names {
            info!("GPU: {}", device_name);
        }
        if gpu_names.is_empty() {
            info!("No NVIDIA GPUs detected");
        }
    }

    Ok(gpu_names)
}

#[cfg(windows)]
pub fn gpu_info(log_info: bool) -> anyhow::Result<Vec<String>> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_DESC1, IDXGIFactory1};
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1().map_err(|e| anyhow::anyhow!(e))? };
    let mut adapter_index = 0;
    let mut gpu_names = Vec::new();

    while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
        let desc: DXGI_ADAPTER_DESC1 =
            unsafe { adapter.GetDesc1().map_err(|e| anyhow::anyhow!(e))? };
        let device_name = String::from_utf16_lossy(&desc.Description);
        if !device_name.contains("Microsoft") {
            let mut device_name = String::from_utf16_lossy(&desc.Description);
            device_name = device_name.replace('\0', "");
            device_name = device_name.trim().to_string();
            device_name = device_name.split_whitespace().collect::<Vec<_>>().join(" ");
            if !gpu_names.contains(&device_name) {
                gpu_names.push(device_name.clone());
            }
        }
        adapter_index += 1;
    }

    gpu_names.sort();
    if log_info {
        for device_name in &gpu_names {
            info!("GPU: {}", device_name);
        }
    }

    Ok(gpu_names)
}

/// GPU utilization metrics
#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub utilization_percent: Option<f32>,
    pub memory_used_mb: Option<u64>,
    pub memory_total_mb: Option<u64>,
    pub temperature_celsius: Option<f32>,
}

/// Query GPU utilization for a specific GPU index
#[cfg(not(windows))]
pub fn gpu_utilization(gpu_index: usize) -> Option<f32> {
    use std::process::Command;

    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=utilization.gpu")
        .arg("--format=csv,noheader,nounits")
        .arg("--id")
        .arg(gpu_index.to_string())
        .output()
        .ok()
        .filter(|o| o.status.success())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f32>().ok()
}

#[cfg(windows)]
pub fn gpu_utilization(_gpu_index: usize) -> Option<f32> {
    // Windows DirectX doesn't provide direct utilization metrics via DXGI
    // This would require WMI or other APIs, which is more complex
    // For now, return None - can be enhanced later
    // jlk: note.
    None
}

/// Query GPU memory usage for a specific GPU index
#[cfg(not(windows))]
pub fn gpu_memory_usage(gpu_index: usize) -> Option<(u64, u64)> {
    use std::process::Command;

    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=memory.used,memory.total")
        .arg("--format=csv,noheader,nounits")
        .arg("--id")
        .arg(gpu_index.to_string())
        .output()
        .ok()
        .filter(|o| o.status.success())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() >= 2 {
        let used = parts[0].trim().parse::<u64>().ok()?;
        let total = parts[1].trim().parse::<u64>().ok()?;
        Some((used, total))
    } else {
        None
    }
}

#[cfg(windows)]
pub fn gpu_memory_usage(gpu_index: usize) -> Option<(u64, u64)> {
    // jlk: so it can get gpu mem on windows, but not gpu usage??
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1().ok()? };
    let adapter = unsafe { factory.EnumAdapters1(gpu_index as u32).ok()? };

    // Get adapter description for total memory
    // Note: Windows DXGI doesn't easily provide used memory without IDXGIAdapter3
    // which requires additional setup. For now, we'll return total memory only.
    if let Ok(desc) = unsafe { adapter.GetDesc1() } {
        // DedicatedVideoMemory is in bytes, convert to MB
        let total_mb = (desc.DedicatedVideoMemory / (1024 * 1024)) as u64;
        // We don't have used memory easily available via DXGI without IDXGIAdapter3
        // Return total with 0 used as a placeholder
        Some((0, total_mb))
    } else {
        None
    }
}

/// Query GPU temperature for a specific GPU index
#[cfg(not(windows))]
pub fn gpu_temperature(gpu_index: usize) -> Option<f32> {
    use std::process::Command;

    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=temperature.gpu")
        .arg("--format=csv,noheader,nounits")
        .arg("--id")
        .arg(gpu_index.to_string())
        .output()
        .ok()
        .filter(|o| o.status.success())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f32>().ok()
}

#[cfg(windows)]
pub fn gpu_temperature(_gpu_index: usize) -> Option<f32> {
    // Windows DirectX doesn't provide temperature via DXGI
    // This would require WMI or other APIs
    None
}

/// Get comprehensive GPU metrics for a specific GPU index
pub fn get_gpu_metrics(gpu_index: usize) -> GpuMetrics {
    GpuMetrics {
        utilization_percent: gpu_utilization(gpu_index),
        memory_used_mb: gpu_memory_usage(gpu_index).map(|(used, _)| used),
        memory_total_mb: gpu_memory_usage(gpu_index).map(|(_, total)| total),
        temperature_celsius: gpu_temperature(gpu_index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_cuda_gpu_info() {
        gpu_info(true).unwrap();
    }

    #[test]
    fn print_cpu_info() {
        cpu_info().unwrap()
    }
}
