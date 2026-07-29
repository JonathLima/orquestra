use orquestra_plan::{Plan, Ticket};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::{SkillInfo, SkillInventory, SkillStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatch {
    pub skill_id: String,
    pub skill_name: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatchReport {
    pub ticket_id: String,
    pub selected_skill: Option<String>,
    pub matches: Vec<SkillMatch>,
    pub unresolved: bool,
}

pub fn match_plan(plan: &Plan, inventory: &SkillInventory) -> Vec<SkillMatchReport> {
    plan.tickets
        .iter()
        .map(|ticket| match_ticket(ticket, inventory))
        .collect()
}

pub fn match_ticket(ticket: &Ticket, inventory: &SkillInventory) -> SkillMatchReport {
    let mut matches = inventory
        .skills
        .iter()
        .filter(|skill| skill.status == SkillStatus::Active)
        .filter_map(|skill| score_skill(ticket, skill))
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.skill_name.cmp(&right.skill_name))
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    matches.truncate(5);

    let selected_skill = matches.first().map(|m| m.skill_name.clone());

    SkillMatchReport {
        ticket_id: ticket.id.clone(),
        selected_skill,
        unresolved: matches.is_empty(),
        matches,
    }
}

fn score_skill(ticket: &Ticket, skill: &SkillInfo) -> Option<SkillMatch> {
    let mut score = 0.0;
    let mut reasons = Vec::new();
    let query_terms = ticket_terms(ticket);
    let skill_terms = skill_terms(skill);

    if domain_conflict(&query_terms, &skill_terms, skill) {
        return None;
    }

    if let Some(assigned) = &ticket.assigned_skill
        && (assigned.eq_ignore_ascii_case(&skill.name) || assigned.eq_ignore_ascii_case(&skill.id))
    {
        score += 1.0;
        reasons.push("exact assigned skill match".to_string());
    }

    let capability_hits = overlap_count(&ticket.preferred_capabilities, &skill.capabilities);
    if capability_hits > 0 {
        score += (capability_hits as f64 * 0.5).min(1.0);
        reasons.push(format!("{capability_hits} preferred capability match(es)"));
    }

    let semantic_capability_hits = ticket
        .preferred_capabilities
        .iter()
        .filter(|capability| {
            let terms = tokenize(capability);
            !terms.is_empty() && terms.iter().all(|term| skill_terms.contains(term))
        })
        .count()
        .saturating_sub(capability_hits);
    if semantic_capability_hits > 0 {
        score += (semantic_capability_hits as f64 * 0.3).min(0.6);
        reasons.push(format!(
            "{semantic_capability_hits} capability term alignment(s)"
        ));
    }

    let keyword_hits = query_terms.intersection(&skill_terms).count();
    if keyword_hits > 0 {
        score += (keyword_hits as f64 / query_terms.len().max(1) as f64).min(0.19);
        reasons.push(format!("{keyword_hits} keyword match(es)"));
    }

    if score <= 0.0 {
        return None;
    }

    Some(SkillMatch {
        skill_id: skill.id.clone(),
        skill_name: skill.name.clone(),
        score: (score * 100.0).round() / 100.0,
        reasons,
    })
}

fn domain_conflict(
    ticket_terms: &BTreeSet<String>,
    skill_terms: &BTreeSet<String>,
    skill: &SkillInfo,
) -> bool {
    const SPECIALIZED_DOMAINS: &[&[&str]] = &[
        &["roblox", "luau"],
        &["godot", "gdscript"],
        &["unity", "monobehaviour"],
        &["phaser", "pixijs", "pygame", "love2d"],
    ];
    if SPECIALIZED_DOMAINS.iter().any(|markers| {
        markers.iter().any(|marker| skill_terms.contains(*marker))
            && !markers.iter().any(|marker| ticket_terms.contains(*marker))
    }) {
        return true;
    }

    let skill_is_game_specific = skill_terms.contains("game")
        && [
            "game", "player", "enemy", "sprite", "level", "save", "physics",
        ]
        .iter()
        .any(|marker| skill_terms.contains(*marker));
    if skill_is_game_specific
        && !["game", "player", "enemy", "sprite", "level", "gameplay"]
            .iter()
            .any(|marker| ticket_terms.contains(*marker))
    {
        return true;
    }

    let ticket_is_backend = ["backend", "api", "server", "express"]
        .iter()
        .any(|marker| ticket_terms.contains(*marker));
    let ticket_is_frontend = ["frontend", "website", "dashboard", "component", "ui", "ux"]
        .iter()
        .any(|marker| ticket_terms.contains(*marker));
    let skill_is_frontend_only = skill
        .description
        .to_ascii_lowercase()
        .contains("not for backend")
        || (skill_terms.contains("frontend")
            && skill_terms.contains("ui")
            && !skill_terms.contains("backend"));

    ticket_is_backend && !ticket_is_frontend && skill_is_frontend_only
}

fn overlap_count(left: &[String], right: &[String]) -> usize {
    let right_terms = right.iter().map(|s| normalize(s)).collect::<BTreeSet<_>>();
    left.iter()
        .map(|s| normalize(s))
        .filter(|s| right_terms.contains(s))
        .count()
}

fn ticket_terms(ticket: &Ticket) -> BTreeSet<String> {
    let mut text = format!("{} {}", ticket.title, ticket.objective);
    text.push(' ');
    text.push_str(&ticket.acceptance_criteria.join(" "));
    text.push(' ');
    text.push_str(&ticket.preferred_capabilities.join(" "));
    tokenize(&text)
}

fn skill_terms(skill: &SkillInfo) -> BTreeSet<String> {
    let mut text = format!("{} {}", skill.name, skill.description);
    text.push(' ');
    text.push_str(&skill.capabilities.join(" "));
    tokenize(&text)
}

fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(normalize)
        .filter(|token| token.len() >= 3)
        .collect()
}

fn normalize(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Provenance, ScanSource, TrustLevel};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn skill(name: &str, capabilities: &[&str]) -> SkillInfo {
        SkillInfo {
            id: name.to_string(),
            name: name.to_string(),
            description: format!("{name} development skill"),
            version: None,
            scope: "global".to_string(),
            source_path: PathBuf::from(format!("/tmp/{name}/SKILL.md")),
            hash: "sha256:test".to_string(),
            trust: TrustLevel::UserGlobal,
            status: SkillStatus::Active,
            capabilities: capabilities.iter().map(|s| s.to_string()).collect(),
            metadata: HashMap::new(),
            provenance: Provenance::Local,
            inspected_at: Utc::now(),
        }
    }

    fn inventory() -> SkillInventory {
        SkillInventory {
            schema_version: 1,
            generated_at: Utc::now(),
            sources: vec![ScanSource {
                scope: "global".to_string(),
                path: PathBuf::from("/tmp/skills"),
            }],
            skills: vec![skill("rust-backend", &["rust", "cli"])],
        }
    }

    #[test]
    fn exact_assigned_skill_wins() {
        let ticket = Ticket {
            id: "T1".to_string(),
            title: "Build CLI".to_string(),
            objective: "Implement Rust command".to_string(),
            acceptance_criteria: vec!["tests pass".to_string()],
            blocked_by: vec![],
            preferred_capabilities: vec!["rust".to_string()],
            assigned_skill: Some("rust-backend".to_string()),
            model_policy: None,
            model_recommendation: None,
            verification: Default::default(),
        };

        let report = match_ticket(&ticket, &inventory());

        assert_eq!(report.selected_skill, Some("rust-backend".to_string()));
        assert!(!report.unresolved);
        assert!(report.matches[0].score >= 1.0);
    }

    #[test]
    fn inactive_skills_are_ignored() {
        let mut inv = inventory();
        inv.skills[0].status = SkillStatus::Pending;
        let ticket = Ticket {
            id: "T1".to_string(),
            title: "Build CLI".to_string(),
            objective: "Implement Rust command".to_string(),
            acceptance_criteria: vec!["tests pass".to_string()],
            blocked_by: vec![],
            preferred_capabilities: vec!["rust".to_string()],
            assigned_skill: None,
            model_policy: None,
            model_recommendation: None,
            verification: Default::default(),
        };

        let report = match_ticket(&ticket, &inv);

        assert!(report.unresolved);
        assert!(report.matches.is_empty());
    }
}
