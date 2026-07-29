use crate::cli::AdapterAction;
use crate::output::{OutputData, print_output};
use orquestra_adapters::{Confidence, all_adapters, detect_all_adapters, get_adapter};
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct AdapterEntry {
    name: String,
    detected: bool,
    version: Option<String>,
    capabilities: Vec<String>,
    skills_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct AdapterListOutput {
    adapters: Vec<AdapterEntry>,
}

impl OutputData for AdapterListOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str("Known adapters:\n\n");
        for a in &self.adapters {
            let status = if a.detected { "installed" } else { "not found" };
            let caps = a.capabilities.join(", ");
            let sd = a
                .skills_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            out.push_str(&format!("  {}  ({})\n", a.name, status));
            out.push_str(&format!("       capabilities: {caps}\n"));
            if !sd.is_empty() {
                out.push_str(&format!("       skills dir:    {sd}\n"));
            }
            out.push('\n');
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct DetectedHost {
    name: String,
    version: String,
    binary_path: PathBuf,
    skills_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    confidence: String,
}

#[derive(Debug, Serialize)]
struct AdapterDetectOutput {
    detected: Option<DetectedHost>,
    all_adapters: Vec<AdapterEntry>,
}

impl OutputData for AdapterDetectOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str("Detected CLIs:\n\n");
        match &self.detected {
            Some(host) => {
                out.push_str(&format!("  Primary: {} v{}\n", host.name, host.version));
                out.push_str(&format!("  Binary:  {}\n", host.binary_path.display()));
                out.push_str(&format!("  Confidence: {}\n", host.confidence));
                if let Some(sd) = &host.skills_dir {
                    out.push_str(&format!("  Skills:  {}\n", sd.display()));
                }
                if let Some(cd) = &host.config_dir {
                    out.push_str(&format!("  Config:  {}\n", cd.display()));
                }
            }
            None => {
                out.push_str("  (no supported CLI detected)\n");
            }
        }
        out.push_str("\nAll adapters:\n");
        for a in &self.all_adapters {
            let status = if a.detected { "installed" } else { "not found" };
            out.push_str(&format!("  {}  ({})\n", a.name, status));
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct ToolMapping {
    abstract_name: String,
    native_name: String,
}

#[derive(Debug, Serialize)]
struct AdapterPaths {
    skills_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    project_skills_dir: Option<PathBuf>,
    agents_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CapWithDesc {
    name: String,
    description: String,
}

#[derive(Debug, Serialize)]
struct AdapterInspectOutput {
    name: String,
    detected: bool,
    capabilities: Vec<CapWithDesc>,
    tool_map: Vec<ToolMapping>,
    paths: AdapterPaths,
    notes: Vec<String>,
}

impl OutputData for AdapterInspectOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Adapter: {}\n\n", self.name));
        out.push_str(&format!(
            "Status: {}\n\n",
            if self.detected {
                "installed"
            } else {
                "not found"
            }
        ));

        out.push_str("Capabilities:\n");
        for cap in &self.capabilities {
            out.push_str(&format!("  {}  -- {}\n", cap.name, cap.description));
        }

        out.push_str("\nTool map:\n");
        for t in &self.tool_map {
            out.push_str(&format!("  {} -> {}\n", t.abstract_name, t.native_name));
        }

        out.push_str("\nPaths:\n");
        if let Some(p) = &self.paths.skills_dir {
            out.push_str(&format!("  skills:       {}\n", p.display()));
        }
        if let Some(p) = &self.paths.config_dir {
            out.push_str(&format!("  config:       {}\n", p.display()));
        }
        if let Some(p) = &self.paths.agents_dir {
            out.push_str(&format!("  agents:       {}\n", p.display()));
        }

        if !self.notes.is_empty() {
            out.push_str("\nNotes:\n");
            for n in &self.notes {
                out.push_str(&format!("  {n}\n"));
            }
        }
        out
    }
}

pub fn run(action: &AdapterAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        AdapterAction::List => run_list(output),
        AdapterAction::Detect => run_detect(output),
        AdapterAction::Inspect { host } => run_inspect(host, output),
    }
}

fn run_list(output: &OutputFormat) -> Result<(), OrquestraError> {
    let detected = detect_all_adapters();
    let entries: Vec<AdapterEntry> = all_adapters()
        .into_iter()
        .map(|a| {
            let name = a.name().to_string();
            let caps: Vec<String> = a.capabilities().iter().map(|c| c.to_string()).collect();
            let info = detected.iter().find(|d| d.name == name);
            let skills_dir = info.and_then(|i| i.skills_dir.clone());
            AdapterEntry {
                name,
                detected: info.is_some(),
                version: info.map(|i| i.version.clone()),
                capabilities: caps,
                skills_dir,
            }
        })
        .collect();

    print_output(&AdapterListOutput { adapters: entries }, output);
    Ok(())
}

fn run_detect(output: &OutputFormat) -> Result<(), OrquestraError> {
    let detected = detect_all_adapters();

    let all_adapters_info: Vec<AdapterEntry> = all_adapters()
        .into_iter()
        .map(|a| {
            let name = a.name().to_string();
            let caps: Vec<String> = a.capabilities().iter().map(|c| c.to_string()).collect();
            let info = detected.iter().find(|d| d.name == name);
            let skills_dir = info.and_then(|i| i.skills_dir.clone());
            AdapterEntry {
                name,
                detected: info.is_some(),
                version: info.map(|i| i.version.clone()),
                capabilities: caps,
                skills_dir,
            }
        })
        .collect();

    let confidence_rank = |c: &Confidence| match c {
        Confidence::High => 0,
        Confidence::Medium => 1,
        Confidence::Low => 2,
    };
    let mut detected_sorted: Vec<_> = detected;
    detected_sorted.sort_by_key(|d| confidence_rank(&d.confidence));
    let primary = detected_sorted.into_iter().next().map(|info| DetectedHost {
        name: info.name.to_string(),
        version: info.version,
        binary_path: info.binary_path,
        skills_dir: info.skills_dir,
        config_dir: info.config_dir,
        confidence: format!("{:?}", info.confidence),
    });

    print_output(
        &AdapterDetectOutput {
            detected: primary,
            all_adapters: all_adapters_info,
        },
        output,
    );
    Ok(())
}

fn run_inspect(host: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let adapter = get_adapter(host).ok_or_else(|| {
        OrquestraError::from(format!(
            "Unknown adapter: {host}. Known: codex, claude-code, opencode, antigravity"
        ))
    })?;

    let detected = detect_all_adapters();
    let info = detected.iter().find(|d| d.name == host);

    let caps: Vec<CapWithDesc> = adapter
        .capabilities()
        .iter()
        .map(|c| CapWithDesc {
            name: c.to_string(),
            description: c.description().to_string(),
        })
        .collect();

    let tool_map: Vec<ToolMapping> = adapter
        .tool_map()
        .into_iter()
        .map(|(k, v)| ToolMapping {
            abstract_name: k.to_string(),
            native_name: v.to_string(),
        })
        .collect();

    let paths = AdapterPaths {
        skills_dir: info.and_then(|i| i.skills_dir.clone()),
        config_dir: info.and_then(|i| i.config_dir.clone()),
        project_skills_dir: info.and_then(|i| i.project_skills_dir.clone()),
        agents_dir: info.and_then(|i| i.agents_dir.clone()),
    };

    let notes: Vec<String> = adapter.notes().iter().map(|s| s.to_string()).collect();

    print_output(
        &AdapterInspectOutput {
            name: host.to_string(),
            detected: info.is_some(),
            capabilities: caps,
            tool_map,
            paths,
            notes,
        },
        output,
    );
    Ok(())
}
