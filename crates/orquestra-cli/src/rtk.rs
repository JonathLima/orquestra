use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RtkStatus {
    pub installed: bool,
    pub version: Option<String>,
}

pub fn detect() -> RtkStatus {
    let installed = which::which("rtk").is_ok();
    let version = if installed {
        std::process::Command::new("rtk")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
    } else {
        None
    };
    RtkStatus { installed, version }
}
