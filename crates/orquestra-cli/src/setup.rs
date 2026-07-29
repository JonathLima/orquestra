use crate::embedded_skills;
use crate::output::{OutputData, print_output};
use crate::rtk::RtkStatus;
use orquestra_adapters::{FileOperation, get_adapter};
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const ORQUESTRA_DISCOVERY_BLOCK: &str = r#"
## Orquestra Skills

The following skills are available for Orquestra workflows:

- `orquestra-init`: Public entry point. Use it when the user invokes `orquestra-init <problem>` and drive discovery, research, planning, execution, verification, and delivery.
- `orquestra-orchestrator`: Runtime support for DAG execution, subagent dispatching, and strict verification.
- `orquestra-planner`: Use when generating a structured DAG of tickets with dependencies and parallel Waves from high-level requirements.
- `orquestra-router`: Use when routing tickets to domain skills, detecting technical gaps, and triggering WIE research before subagent dispatch.
- `orquestra-verifier`: Use when evaluating subagent output against project DNA checklists before marking tickets complete.
- `orquestra-grill`: Use for the initial adaptive interview. Inspect local facts first, then ask one question for the confidence gap that most needs evidence.
- `orquestra-grill-with-docs`: Use when an existing project needs documentary context. Obtain one consolidated consent for document paths and types before reading any content.

When the user invokes `orquestra-init` or asks Orquestra to solve a problem, load `orquestra-init` as the single public workflow. It selects the appropriate grill engine and the supporting runtime skills.
"#;

const OPENCODE_INIT_COMMAND: &str = r#"---
description: Run the complete autonomous Orquestra workflow
---
Load the `orquestra-init` skill and execute its protocol end to end.

Initial problem:
$ARGUMENTS
"#;

#[derive(Debug, Serialize)]
struct SetupOutput {
    host: String,
    target_skills_dir: PathBuf,
    operations: Vec<OperationDisplay>,
    rtk: RtkStatus,
}

#[derive(Debug, Serialize)]
struct OperationDisplay {
    op_type: String,
    path: String,
    detail: String,
}

impl OutputData for SetupOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Install plan for {}\n", self.host));
        out.push_str(&format!("Target: {}\n\n", self.target_skills_dir.display()));
        for op in &self.operations {
            out.push_str(&format!("  {}  {}  ({})\n", op.op_type, op.path, op.detail));
        }
        if self.rtk.installed {
            let ver = self.rtk.version.as_deref().unwrap_or("unknown");
            out.push_str(&format!("\nRTK: installed ({ver})\n"));
        } else {
            out.push_str("\nRTK: not installed (install at https://rtk-ai.app)\n");
        }
        out
    }
}

fn build_operations(
    plan: &orquestra_adapters::InstallPlan,
    dry_run: bool,
) -> Result<Vec<OperationDisplay>, OrquestraError> {
    let mut ops = vec![];
    let skills: BTreeMap<String, &str> = embedded_skills::all_skills()
        .into_iter()
        .map(|s| (s.name, s.content))
        .collect();

    for op in &plan.operations {
        match op {
            FileOperation::CreateDir { path } => {
                if !dry_run {
                    std::fs::create_dir_all(path).map_err(|e| {
                        OrquestraError::from(format!(
                            "Failed to create dir {}: {e}",
                            path.display()
                        ))
                    })?;
                }
                ops.push(OperationDisplay {
                    op_type: "CREATE".to_string(),
                    path: path.display().to_string(),
                    detail: "directory".to_string(),
                });
            }
            FileOperation::CopySkill {
                skill_name,
                target_dir,
            } => {
                let content = skills
                    .get(skill_name.as_str())
                    .ok_or_else(|| OrquestraError::from(format!("Unknown skill: {skill_name}")))?;
                let target_file = target_dir.join("SKILL.md");
                if !dry_run {
                    std::fs::write(&target_file, content).map_err(|e| {
                        OrquestraError::from(format!(
                            "Failed to write {}: {e}",
                            target_file.display()
                        ))
                    })?;
                }
                ops.push(OperationDisplay {
                    op_type: "COPY".to_string(),
                    path: target_file.display().to_string(),
                    detail: format!("{skill_name} SKILL.md"),
                });
            }
            FileOperation::WriteToolsJson {
                target_file,
                tool_map,
            } => {
                if !dry_run {
                    let json = serde_json::to_string_pretty(&tool_map).map_err(|e| {
                        OrquestraError::from(format!("Failed to serialize tools.json: {e}"))
                    })?;
                    std::fs::write(target_file, &json).map_err(|e| {
                        OrquestraError::from(format!(
                            "Failed to write {}: {e}",
                            target_file.display()
                        ))
                    })?;
                }
                ops.push(OperationDisplay {
                    op_type: "WRITE".to_string(),
                    path: target_file.display().to_string(),
                    detail: "tools.json".to_string(),
                });
            }
            FileOperation::WriteDiscoveryBlock { target_file } => {
                let block = ORQUESTRA_DISCOVERY_BLOCK.trim();
                if !dry_run {
                    let existing = match std::fs::read_to_string(target_file) {
                        Ok(content) => content,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                        Err(error) => {
                            return Err(OrquestraError::from(format!(
                                "Failed to read {} before writing discovery block: {error}",
                                target_file.display()
                            )));
                        }
                    };
                    if existing.contains("orquestra-orchestrator") {
                        ops.push(OperationDisplay {
                            op_type: "SKIP".to_string(),
                            path: target_file.display().to_string(),
                            detail: "already has Orquestra discovery".to_string(),
                        });
                        continue;
                    }
                    let content = if existing.is_empty() {
                        format!("{block}\n")
                    } else {
                        format!("{existing}\n{block}\n")
                    };
                    std::fs::write(target_file, &content).map_err(|e| {
                        OrquestraError::from(format!(
                            "Failed to write {}: {e}",
                            target_file.display()
                        ))
                    })?;
                }
                ops.push(OperationDisplay {
                    op_type: "WRITE".to_string(),
                    path: target_file.display().to_string(),
                    detail: "AGENTS.md discovery block".to_string(),
                });
            }
        }
    }
    Ok(ops)
}

fn install_host_entrypoint(
    host: &str,
    dry_run: bool,
    operations: &mut Vec<OperationDisplay>,
) -> Result<(), OrquestraError> {
    if host != "opencode" {
        return Ok(());
    }

    let command_dir = PathBuf::from(".opencode").join("commands");
    let command_file = command_dir.join("orquestra-init.md");
    if !dry_run {
        std::fs::create_dir_all(&command_dir).map_err(|error| {
            OrquestraError::from(format!(
                "Failed to create OpenCode command directory {}: {error}",
                command_dir.display()
            ))
        })?;
        std::fs::write(&command_file, OPENCODE_INIT_COMMAND).map_err(|error| {
            OrquestraError::from(format!(
                "Failed to write OpenCode command {}: {error}",
                command_file.display()
            ))
        })?;
    }
    operations.push(OperationDisplay {
        op_type: "WRITE".to_string(),
        path: command_file.display().to_string(),
        detail: "OpenCode /orquestra-init command".to_string(),
    });
    Ok(())
}

pub fn run(host: &str, dry_run: bool, output: &OutputFormat) -> Result<(), OrquestraError> {
    let adapter = get_adapter(host).ok_or_else(|| {
        OrquestraError::from(format!(
            "Unknown host: {host}. Available: codex, claude-code, opencode, antigravity"
        ))
    })?;

    let skills = embedded_skills::all_skills();
    let plan = adapter.install_plan(&skills);

    let mut operations = build_operations(&plan, dry_run)?;
    install_host_entrypoint(&plan.host, dry_run, &mut operations)?;
    let rtk = crate::rtk::detect();

    print_output(
        &SetupOutput {
            host: plan.host.clone(),
            target_skills_dir: plan.target_skills_dir.clone(),
            operations,
            rtk,
        },
        output,
    );

    if !dry_run {
        let config_path = PathBuf::from(".orquestra").join("config.toml");
        if !config_path.exists() {
            let parent = config_path.parent().unwrap();
            std::fs::create_dir_all(parent).map_err(|e| {
                OrquestraError::from(format!("Failed to create .orquestra directory: {e}"))
            })?;
            let default_config = r#"config_version = 1

[init]
min_confidence = 0.95
max_tickets = 8
min_rounds = 3
auto_research = true
max_contradictions = 0
require_primary_source = true

[init.research]
min_sources_per_topic = 5
min_agreement_for_confirmed = 2
min_reliability_score = 0.95
max_research_loops = 3
prefer_official_docs = true
allow_user_override = false
"#;
            std::fs::write(&config_path, default_config).map_err(|e| {
                OrquestraError::from(format!("Failed to write .orquestra/config.toml: {e}"))
            })?;
        }
        println!(
            "\nSetup complete for {}. Skills installed to {}.",
            plan.host,
            plan.target_skills_dir.display()
        );
        println!("RTK hint: run `rtk` to manage skills efficiently.");
        if plan.host == "opencode" {
            println!("OpenCode entry point: /orquestra-init <problem>");
        } else if plan.host == "claude-code" {
            println!("Claude Code entry point: /orquestra-init <problem>");
        } else if plan.host == "codex" {
            println!("Codex entry point: $orquestra-init <problem>");
        } else {
            println!("Antigravity entry point: orquestra-init <problem>");
        }
    }

    Ok(())
}
