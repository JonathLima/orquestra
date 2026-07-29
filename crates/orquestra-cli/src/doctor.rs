use crate::output::OutputData;
use crate::rtk::RtkStatus;
use orquestra_adapters::CliInfo;
use orquestra_core::config::{Config, home_dir};
use orquestra_core::error::OrquestraError;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct DoctorOutput {
    pub version: String,
    pub config: Config,
    pub clis: Vec<CliInfo>,
    pub skills_count: SkillsCount,
    pub project_status: ProjectStatus,
    pub rtk: RtkStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityReport>,
}

#[derive(Debug, Serialize)]
pub struct SkillsCount {
    pub agents_skills: usize,
    pub local_skills: usize,
    pub pending_candidates: usize,
}

#[derive(Debug, Serialize)]
pub struct ProjectStatus {
    pub has_orquestra_dir: bool,
    pub has_config: bool,
}

#[derive(Debug, Serialize)]
pub struct SecurityReport {
    pub external_brain: String,
    pub proxy: String,
    pub redaction: String,
    pub allowed_write_roots: Vec<String>,
}

impl SkillsCount {
    fn new() -> Self {
        let agents_path = home_dir().join(".agents").join("skills");
        let local_path = Path::new(".orquestra").join("skills");

        let agents_skills = count_skill_dirs(&agents_path, false);
        let local_skills = count_skill_dirs(&local_path, true);
        let pending_candidates = count_skill_dirs(&local_path.join("_pending"), false);

        Self {
            agents_skills,
            local_skills,
            pending_candidates,
        }
    }
}

fn count_skill_dirs(path: &Path, skip_pending: bool) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    if skip_pending && entry.file_name().to_str() == Some("_pending") {
                        return false;
                    }
                    entry.path().join("SKILL.md").is_file()
                })
                .count()
        })
        .unwrap_or(0)
}

impl ProjectStatus {
    fn new() -> Self {
        let orquestra_dir = Path::new(".orquestra");
        let config_path = orquestra_dir.join("config.toml");
        Self {
            has_orquestra_dir: orquestra_dir.exists(),
            has_config: config_path.exists(),
        }
    }
}

pub fn run(config: &Config, include_security: bool) -> Result<DoctorOutput, OrquestraError> {
    let clis = orquestra_adapters::detect_all_adapters();
    let skills_count = SkillsCount::new();
    let project_status = ProjectStatus::new();
    let rtk = crate::rtk::detect();

    Ok(DoctorOutput {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config: config.clone(),
        clis,
        skills_count,
        project_status,
        rtk,
        security: include_security.then(|| security_report(config)),
    })
}

fn security_report(config: &Config) -> SecurityReport {
    let mut roots = config
        .security
        .allowed_write_roots
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if roots.is_empty() {
        roots.push(
            std::env::current_dir()
                .unwrap_or_default()
                .display()
                .to_string(),
        );
    }
    SecurityReport {
        external_brain: if config.security.allow_external_brain {
            "enabled".to_string()
        } else {
            "disabled".to_string()
        },
        proxy: if config.security.allow_proxy {
            "enabled".to_string()
        } else {
            "disabled".to_string()
        },
        redaction: if config.security.redact_secrets {
            "enabled".to_string()
        } else {
            "disabled".to_string()
        },
        allowed_write_roots: roots,
    }
}

impl OutputData for DoctorOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Orquestra v{}\n\n", self.version));

        out.push_str("## Config\n");
        out.push_str(&format!("  Output:  {}\n", self.config.output));
        out.push_str(&format!("  Log:     {}\n", self.config.log_level));

        out.push_str("\n## CLIs detected\n");
        if self.clis.is_empty() {
            out.push_str("  (none detected)\n");
        } else {
            for cli in &self.clis {
                let sd = cli
                    .skills_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(default)".to_string());
                out.push_str(&format!("  {}: {} ({})\n", cli.name, cli.version, sd));
            }
        }

        out.push_str("\n## Skills\n");
        out.push_str(&format!(
            "  ~/.agents/skills/:  {}\n",
            self.skills_count.agents_skills
        ));
        out.push_str(&format!(
            "  .orquestra/skills/: {}\n",
            self.skills_count.local_skills
        ));
        out.push_str(&format!(
            "  .orquestra/skills/_pending/: {}\n",
            self.skills_count.pending_candidates
        ));

        out.push_str("\n## Project\n");
        out.push_str(&format!(
            "  .orquestra/:  {}\n",
            if self.project_status.has_orquestra_dir {
                "present"
            } else {
                "missing"
            }
        ));
        out.push_str(&format!(
            "  config.toml: {}\n",
            if self.project_status.has_config {
                "present"
            } else {
                "missing"
            }
        ));

        out.push_str("\n## RTK\n");
        if self.rtk.installed {
            out.push_str(&format!(
                "  installed: {}\n",
                self.rtk.version.as_deref().unwrap_or("unknown")
            ));
        } else {
            out.push_str("  not installed (install at https://rtk-ai.app)\n");
        }

        if let Some(security) = &self.security {
            out.push_str("\n## Security\n");
            out.push_str(&format!("  external BRAIN: {}\n", security.external_brain));
            out.push_str(&format!("  proxy:          {}\n", security.proxy));
            out.push_str(&format!("  redaction:      {}\n", security.redaction));
            out.push_str("  write roots:\n");
            for root in &security.allowed_write_roots {
                out.push_str(&format!("    {root}\n"));
            }
        }

        out
    }
}
