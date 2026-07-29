use crate::output::{OutputData, print_output};
use orquestra_adapters::{Capability, get_adapter};
use orquestra_core::config::{Config, OutputFormat};
use orquestra_core::error::OrquestraError;
use serde::Serialize;
use std::process::{Command, Stdio};

#[derive(Debug, Serialize)]
struct ProxyDoctorOutput {
    host: String,
    detected: bool,
    binary: Option<String>,
    capabilities: Vec<String>,
    policy: String,
}

impl OutputData for ProxyDoctorOutput {
    fn render_human(&self) -> String {
        let binary = self.binary.as_deref().unwrap_or("(not detected)");
        format!(
            "Proxy host: {}\nDetected: {}\nBinary: {}\nPolicy: {}\nCapabilities: {}",
            self.host,
            self.detected,
            binary,
            self.policy,
            self.capabilities.join(", ")
        )
    }
}

pub fn run(
    host: &str,
    args: &[String],
    config: &Config,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    if host == "doctor" {
        let target = args
            .first()
            .ok_or_else(|| OrquestraError::from("Usage: orquestra proxy doctor <host>"))?;
        return run_doctor(target, config, output);
    }
    run_proxy(host, args, config)
}

fn run_doctor(host: &str, config: &Config, output: &OutputFormat) -> Result<(), OrquestraError> {
    let adapter = get_adapter(host)
        .ok_or_else(|| OrquestraError::from(format!("Unknown proxy host: {host}")))?;
    let detected = adapter.detect()?;
    let capabilities = adapter
        .capabilities()
        .into_iter()
        .map(capability_name)
        .collect::<Vec<_>>();
    print_output(
        &ProxyDoctorOutput {
            host: host.to_string(),
            detected: detected.is_some(),
            binary: detected.map(|info| info.binary_path.display().to_string()),
            capabilities,
            policy: if config.security.allow_proxy {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
        },
        output,
    );
    Ok(())
}

fn run_proxy(host: &str, args: &[String], config: &Config) -> Result<(), OrquestraError> {
    if !config.security.allow_proxy {
        return Err(OrquestraError::from(
            "Proxy execution is disabled by policy",
        ));
    }
    let adapter = get_adapter(host)
        .ok_or_else(|| OrquestraError::from(format!("Unknown proxy host: {host}")))?;
    let info = adapter
        .detect()?
        .ok_or_else(|| OrquestraError::from(format!("Host CLI '{host}' not detected")))?;
    let mut child = Command::new(&info.binary_path)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| OrquestraError::from(format!("Cannot start proxy host: {error}")))?;
    let status = child
        .wait()
        .map_err(|error| OrquestraError::from(format!("Cannot wait for proxy host: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(OrquestraError::ProcessExit(status.code().unwrap_or(1)))
    }
}

fn capability_name(capability: Capability) -> String {
    match capability {
        Capability::Subagents => "subagents",
        Capability::NonInteractive => "non-interactive",
        Capability::Hooks => "hooks",
        Capability::InstructionsOnly => "instructions-only",
        Capability::FileSystem => "file-system",
        Capability::WebSearch => "web-search",
    }
    .to_string()
}
