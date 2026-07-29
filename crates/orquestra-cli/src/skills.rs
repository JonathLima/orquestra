use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::{Plan, Ticket};
use orquestra_skills::{
    SkillMatchReport, default_scan_sources, match_plan, match_ticket, read_inventory, scan_all,
    write_inventory,
};
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Scan ~/.agents/skills/ and update the inventory
    Scan,
    /// List skills from the current inventory
    List,
    /// Show detailed info for a specific skill
    Info {
        /// Skill name or ID
        name: String,
    },
    /// Re-scan and update inventory only if hashes changed
    Refresh,
    /// Match one ticket or every ticket in a plan against the current inventory
    Match {
        /// JSON file containing either a Ticket or a Plan
        #[arg(long)]
        ticket: String,
    },
}

#[derive(Debug, Serialize)]
struct SkillsListOutput {
    skills: Vec<SkillEntry>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct SkillEntry {
    name: String,
    description: String,
    trust: String,
}

impl OutputData for SkillsListOutput {
    fn render_human(&self) -> String {
        if self.skills.is_empty() {
            return "No skills found.".to_string();
        }
        let mut out = format!("Skills ({} total):\n", self.total);
        for s in &self.skills {
            out.push_str(&format!("  {} — {} ({})\n", s.name, s.description, s.trust));
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct SkillsInfoOutput {
    name: String,
    id: String,
    description: String,
    version: Option<String>,
    scope: String,
    trust: String,
    status: String,
    hash: String,
    path: String,
    provenance: String,
}

impl OutputData for SkillsInfoOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Name:        {}\n", self.name));
        out.push_str(&format!("ID:          {}\n", self.id));
        out.push_str(&format!("Description: {}\n", self.description));
        if let Some(v) = &self.version {
            out.push_str(&format!("Version:     {v}\n"));
        }
        out.push_str(&format!("Scope:       {}\n", self.scope));
        out.push_str(&format!("Trust:       {}\n", self.trust));
        out.push_str(&format!("Status:      {}\n", self.status));
        out.push_str(&format!("Hash:        {}\n", self.hash));
        out.push_str(&format!("Path:        {}\n", self.path));
        out.push_str(&format!("Provenance:  {}\n", self.provenance));
        out
    }
}

pub fn handle_skills(action: &SkillsAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        SkillsAction::Scan => cmd_scan(),
        SkillsAction::List => cmd_list(output),
        SkillsAction::Info { name } => cmd_info(name, output),
        SkillsAction::Refresh => cmd_refresh(),
        SkillsAction::Match { ticket } => cmd_match(ticket, output),
    }
}

fn cmd_scan() -> Result<(), OrquestraError> {
    let config = orquestra_skills::SkillScannerConfig::default();
    let sources = default_scan_sources();
    let skills = scan_all(&sources, &config);
    write_inventory(&skills, &sources)?;
    println!("Scanned {} skill(s). Inventory written.", skills.len());
    Ok(())
}

fn cmd_list(output: &OutputFormat) -> Result<(), OrquestraError> {
    let inv = read_inventory()?;
    match inv {
        None => {
            let data = SkillsListOutput {
                skills: vec![],
                total: 0,
            };
            print_output(&data, output);
            Ok(())
        }
        Some(inventory) => {
            let entries: Vec<SkillEntry> = inventory
                .skills
                .iter()
                .map(|s| SkillEntry {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    trust: format!("{:?}", s.trust),
                })
                .collect();
            let data = SkillsListOutput {
                total: inventory.skills.len(),
                skills: entries,
            };
            print_output(&data, output);
            Ok(())
        }
    }
}

fn cmd_info(name: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let inv = read_inventory()?;
    match inv {
        None => Err(OrquestraError::from(
            "No inventory found. Run 'orquestra skill scan' first.",
        )),
        Some(inventory) => {
            let skill = inventory
                .skills
                .iter()
                .find(|s| s.name == name || s.id == name);
            match skill {
                None => Err(OrquestraError::from(format!(
                    "Skill '{name}' not found in inventory."
                ))),
                Some(s) => {
                    let data = SkillsInfoOutput {
                        name: s.name.clone(),
                        id: s.id.clone(),
                        description: s.description.clone(),
                        version: s.version.clone(),
                        scope: s.scope.clone(),
                        trust: format!("{:?}", s.trust),
                        status: format!("{:?}", s.status),
                        hash: s.hash.clone(),
                        path: s.source_path.display().to_string(),
                        provenance: format!("{:?}", s.provenance),
                    };
                    print_output(&data, output);
                    Ok(())
                }
            }
        }
    }
}

fn cmd_refresh() -> Result<(), OrquestraError> {
    let inv = read_inventory()?;
    let config = orquestra_skills::SkillScannerConfig::default();
    let sources = default_scan_sources();
    let skills = scan_all(&sources, &config);
    match inv {
        None => {
            write_inventory(&skills, &sources)?;
            println!("Scanned {} skill(s). Inventory written.", skills.len());
        }
        Some(existing) => {
            let new_ids: std::collections::HashSet<String> =
                skills.iter().map(|s| s.id.clone()).collect();
            let old_ids: std::collections::HashSet<String> =
                existing.skills.iter().map(|s| s.id.clone()).collect();
            let changed = new_ids.symmetric_difference(&old_ids).count();
            write_inventory(&skills, &sources)?;
            if changed > 0 {
                println!(
                    "Re-scanned. {} changes detected. Inventory updated.",
                    changed
                );
            } else {
                println!("No changes detected. Inventory is up to date.");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct SkillMatchOutput {
    reports: Vec<SkillMatchReport>,
}

impl OutputData for SkillMatchOutput {
    fn render_human(&self) -> String {
        if self.reports.is_empty() {
            return "No tickets to match.".to_string();
        }

        let mut out = String::new();
        for report in &self.reports {
            out.push_str(&format!("Ticket: {}\n", report.ticket_id));
            match &report.selected_skill {
                Some(skill) => out.push_str(&format!("Selected: {skill}\n")),
                None => out.push_str("Selected: unresolved\n"),
            }
            for m in &report.matches {
                out.push_str(&format!(
                    "  {} ({}) score={} [{}]\n",
                    m.skill_name,
                    m.skill_id,
                    m.score,
                    m.reasons.join("; ")
                ));
            }
            if report.unresolved {
                out.push_str("  No active inventory skill matched. Use `orquestra brain adapt` after selecting a local source skill.\n");
            }
        }
        out
    }
}

fn cmd_match(ticket_file: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let inventory = read_inventory()?.ok_or_else(|| {
        OrquestraError::from("No inventory found. Run 'orquestra skill scan' first.")
    })?;
    let content = std::fs::read_to_string(ticket_file)
        .map_err(|error| OrquestraError::from(format!("Cannot read ticket file: {error}")))?;

    let reports = if let Ok(ticket) = serde_json::from_str::<Ticket>(&content) {
        vec![match_ticket(&ticket, &inventory)]
    } else {
        let plan: Plan = serde_json::from_str(&content).map_err(|error| {
            OrquestraError::from(format!("Cannot parse ticket or plan: {error}"))
        })?;
        match_plan(&plan, &inventory)
    };
    print_output(&SkillMatchOutput { reports }, output);
    Ok(())
}
