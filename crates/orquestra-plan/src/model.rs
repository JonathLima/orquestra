use crate::{ModelPolicy, ModelRecommendation, ModelTier, Plan, ReasoningEffort, Ticket};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub host: String,
    pub models: Vec<ModelCatalogEntry>,
    pub generated_at: String,
    pub source_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub model: String,
    pub tier: ModelTier,
    pub reasoning_efforts: Vec<ReasoningEffort>,
    pub web_capable: bool,
    pub notes: String,
}

pub fn catalog_for_host(host: &str) -> Option<ModelCatalog> {
    let models = match host {
        "codex" => vec![
            entry(
                "gpt-5.6-luna",
                ModelTier::Fast,
                vec![ReasoningEffort::Low, ReasoningEffort::Medium],
                true,
                "Efficient OpenAI model for low-risk local coding and documentation tasks.",
            ),
            entry(
                "gpt-5.6-terra",
                ModelTier::Balanced,
                vec![ReasoningEffort::Medium, ReasoningEffort::High],
                true,
                "Balanced OpenAI model for routine implementation and test work.",
            ),
            entry(
                "gpt-5.6-sol",
                ModelTier::Frontier,
                vec![ReasoningEffort::High],
                true,
                "Flagship OpenAI model for high-risk architecture, security, and release decisions.",
            ),
        ],
        "claude-code" => vec![
            entry(
                "claude-3-5-haiku-20241022",
                ModelTier::Fast,
                vec![ReasoningEffort::Low],
                false,
                "Fast Claude model for small low-risk edits when available.",
            ),
            entry(
                "claude-sonnet-4-20250514",
                ModelTier::Balanced,
                vec![ReasoningEffort::Medium, ReasoningEffort::High],
                true,
                "Claude Sonnet model for general coding and review work.",
            ),
            entry(
                "claude-opus-4-1-20250805",
                ModelTier::Frontier,
                vec![ReasoningEffort::High],
                true,
                "Claude Opus model for high-risk reasoning where the user's plan allows access.",
            ),
        ],
        "antigravity" => vec![
            entry(
                "gemini-3.5-flash",
                ModelTier::Fast,
                vec![ReasoningEffort::Low, ReasoningEffort::Medium],
                true,
                "Fast Gemini model for simple Antigravity tasks.",
            ),
            entry(
                "gemini-3.1-pro",
                ModelTier::Balanced,
                vec![ReasoningEffort::Medium, ReasoningEffort::High],
                true,
                "Gemini Pro model for most Antigravity planning and coding work.",
            ),
            entry(
                "claude-opus-4.6-thinking",
                ModelTier::Frontier,
                vec![ReasoningEffort::High],
                true,
                "Frontier Antigravity option when available under the user's plan.",
            ),
        ],
        "opencode" => vec![
            entry(
                "opencode-fast",
                ModelTier::Fast,
                vec![ReasoningEffort::Low],
                false,
                "Provider-configured fast model for low-risk OpenCode tasks.",
            ),
            entry(
                "opencode-balanced",
                ModelTier::Balanced,
                vec![ReasoningEffort::Medium],
                true,
                "Provider-configured balanced model for normal OpenCode work.",
            ),
            entry(
                "opencode-frontier",
                ModelTier::Frontier,
                vec![ReasoningEffort::High],
                true,
                "Provider-configured strongest model for high-risk OpenCode work.",
            ),
        ],
        _ => return None,
    };

    Some(ModelCatalog {
        host: host.to_string(),
        models,
        generated_at: today(),
        source_urls: source_urls_for_host(host),
    })
}

fn source_urls_for_host(host: &str) -> Vec<String> {
    match host {
        "codex" => vec![
            "https://platform.openai.com/docs/models".to_string(),
            "https://developers.openai.com/codex/cli".to_string(),
        ],
        "claude-code" => vec![
            "https://docs.anthropic.com/en/docs/claude-code/settings".to_string(),
            "https://docs.anthropic.com/en/docs/claude-code/cli-reference".to_string(),
        ],
        "antigravity" => vec!["https://developers.google.com/antigravity".to_string()],
        "opencode" => vec!["https://opencode.ai/docs".to_string()],
        _ => Vec::new(),
    }
}

pub fn recommend_for_plan(
    plan: &Plan,
    ticket_id: Option<&str>,
    host_override: Option<&str>,
) -> Result<ModelRecommendation, String> {
    let ticket = select_ticket(plan, ticket_id)?;
    recommend_for_ticket(ticket, plan.model_policy.as_ref(), host_override)
}

pub fn recommend_for_ticket(
    ticket: &Ticket,
    plan_policy: Option<&ModelPolicy>,
    host_override: Option<&str>,
) -> Result<ModelRecommendation, String> {
    let policy = ticket
        .model_policy
        .as_ref()
        .or(plan_policy)
        .cloned()
        .unwrap_or_default();
    let host = host_override
        .map(str::to_string)
        .or_else(|| policy.default_host.clone())
        .unwrap_or_else(|| "manual".to_string());

    let quality_risk = classify_quality_risk(ticket);
    let web_requested = requires_web(ticket);
    let web_required = web_requested && web_research_allowed(&policy);
    let tier = choose_tier(ticket, &policy, quality_risk, web_required);
    let reasoning_effort = match tier {
        ModelTier::Fast => ReasoningEffort::Low,
        ModelTier::Balanced => ReasoningEffort::Medium,
        ModelTier::Frontier => ReasoningEffort::High,
    };
    let (model, source) = match model_for_tier(&host, tier) {
        Some(m) => (m, "catalog:v1".to_string()),
        None => (
            format!("{host}-{}", tier.as_str()),
            "fallback:rule".to_string(),
        ),
    };
    let valid_until = {
        let future = Utc::now()
            .checked_add_signed(chrono::TimeDelta::hours(24))
            .unwrap_or(Utc::now());
        future.date_naive().to_string()
    };
    let estimated_cost_class = match tier {
        ModelTier::Fast => "low",
        ModelTier::Balanced => "medium",
        ModelTier::Frontier => "high",
    }
    .to_string();

    Ok(ModelRecommendation {
        ticket_id: ticket.id.clone(),
        host,
        model,
        tier,
        reasoning_effort,
        web_required,
        estimated_cost_class,
        quality_risk,
        reason: reason(ticket, tier, web_requested, web_required, &policy),
        resolved_at: today(),
        confirmed: false,
        source,
        valid_until,
    })
}

fn entry(
    model: &str,
    tier: ModelTier,
    reasoning_efforts: Vec<ReasoningEffort>,
    web_capable: bool,
    notes: &str,
) -> ModelCatalogEntry {
    ModelCatalogEntry {
        model: model.to_string(),
        tier,
        reasoning_efforts,
        web_capable,
        notes: notes.to_string(),
    }
}

fn select_ticket<'a>(plan: &'a Plan, ticket_id: Option<&str>) -> Result<&'a Ticket, String> {
    match ticket_id {
        Some(id) => plan
            .tickets
            .iter()
            .find(|ticket| ticket.id == id)
            .ok_or_else(|| format!("Ticket '{id}' not found in plan")),
        None if plan.tickets.len() == 1 => Ok(&plan.tickets[0]),
        None => Err(format!(
            "Expected --ticket-id for plan with {} tickets",
            plan.tickets.len()
        )),
    }
}

fn model_for_tier(host: &str, tier: ModelTier) -> Option<String> {
    catalog_for_host(host).and_then(|catalog| {
        catalog
            .models
            .into_iter()
            .find(|entry| entry.tier == tier)
            .map(|entry| entry.model)
    })
}

fn choose_tier(
    ticket: &Ticket,
    policy: &ModelPolicy,
    quality_risk: u8,
    web_required: bool,
) -> ModelTier {
    if quality_risk >= 80
        || web_required
        || ticket.verification.minimum_score >= 0.98
        || contains_any(ticket, HIGH_RISK_TERMS)
    {
        return ModelTier::Frontier;
    }
    if quality_risk >= 45 || policy.quality_target == "max" && contains_any(ticket, MEDIUM_TERMS) {
        return ModelTier::Balanced;
    }
    ModelTier::Fast
}

fn classify_quality_risk(ticket: &Ticket) -> u8 {
    let mut risk = 10;
    if ticket.verification.minimum_score >= 0.98 {
        risk += 35;
    } else if ticket.verification.minimum_score >= 0.95 {
        risk += 10;
    }
    if contains_any(ticket, HIGH_RISK_TERMS) {
        risk += 55;
    }
    if contains_any(ticket, MEDIUM_TERMS) {
        risk += 25;
    }
    if requires_web(ticket) {
        risk += 25;
    }
    risk.min(100)
}

fn requires_web(ticket: &Ticket) -> bool {
    contains_any(
        ticket,
        &["web", "internet", "current", "research", "pesquisa"],
    ) || ticket
        .preferred_capabilities
        .iter()
        .any(|cap| cap.eq_ignore_ascii_case("web-search"))
}

fn web_research_allowed(policy: &ModelPolicy) -> bool {
    policy.allow_web_research && !policy.prefer_local_only
}

fn contains_any(ticket: &Ticket, terms: &[&str]) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        ticket.title,
        ticket.objective,
        ticket.acceptance_criteria.join(" "),
        ticket.preferred_capabilities.join(" ")
    )
    .to_ascii_lowercase();
    terms.iter().any(|term| haystack.contains(term))
}

fn reason(
    ticket: &Ticket,
    tier: ModelTier,
    web_requested: bool,
    web_required: bool,
    policy: &ModelPolicy,
) -> String {
    let mut reasons: Vec<String> = Vec::new();
    if contains_any(ticket, HIGH_RISK_TERMS) {
        reasons.push("high-risk security/release/data-loss terms detected".to_string());
    }
    if ticket.verification.minimum_score >= 0.98 {
        reasons.push("verification threshold is production-critical".to_string());
    }
    if web_required {
        reasons.push("current-date web research is required or requested".to_string());
    } else if web_requested {
        reasons.push(format!(
            "web research requested but policy blocked it (allowWebResearch={}, preferLocalOnly={})",
            policy.allow_web_research, policy.prefer_local_only
        ));
    }
    if reasons.is_empty() {
        reasons.push(
            match tier {
                ModelTier::Fast => "low-risk ticket suitable for token-efficient execution",
                ModelTier::Balanced => "moderate implementation risk requires balanced reasoning",
                ModelTier::Frontier => "frontier reasoning selected for quality protection",
            }
            .to_string(),
        );
    }
    format!(
        "{}. Policy qualityTarget={}, costSensitivity={}.",
        reasons.join("; "),
        policy.quality_target,
        policy.cost_sensitivity
    )
}

fn today() -> String {
    Utc::now().date_naive().to_string()
}

const HIGH_RISK_TERMS: &[&str] = &[
    "security",
    "segurança",
    "auth",
    "secret",
    "token",
    "payment",
    "produção",
    "production",
    "release",
    "publish",
    "data-loss",
    "migration",
    "compliance",
];

const MEDIUM_TERMS: &[&str] = &[
    "architecture",
    "runtime",
    "refactor",
    "integration",
    "debug",
    "verification",
    "reroute",
    "model",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerificationPolicy;

    fn ticket_requesting_web() -> Ticket {
        Ticket {
            id: "T1".to_string(),
            title: "Research model routing".to_string(),
            objective: "Use current web research to validate model policy".to_string(),
            acceptance_criteria: vec!["Current web research reviewed".to_string()],
            blocked_by: vec![],
            preferred_capabilities: vec!["web-search".to_string()],
            assigned_skill: None,
            model_policy: None,
            model_recommendation: None,
            verification: VerificationPolicy::default(),
        }
    }

    fn plan_policy(allow_web_research: bool) -> ModelPolicy {
        ModelPolicy {
            allow_web_research,
            ..ModelPolicy::default()
        }
    }

    #[test]
    fn defaults_block_web_required_for_web_ticket() {
        let ticket = ticket_requesting_web();

        let recommendation =
            recommend_for_ticket(&ticket, None, Some("codex")).expect("recommendation");

        assert!(!recommendation.web_required);
        assert!(
            recommendation
                .reason
                .contains("web research requested but policy blocked it")
        );
        assert!(recommendation.reason.contains("allowWebResearch=false"));
    }

    #[test]
    fn plan_policy_can_enable_web_required_when_defaults_disable_it() {
        let ticket = ticket_requesting_web();
        let policy = plan_policy(true);

        let recommendation =
            recommend_for_ticket(&ticket, Some(&policy), Some("codex")).expect("recommendation");

        assert!(recommendation.web_required);
    }

    #[test]
    fn ticket_policy_overrides_plan_policy_for_web_research() {
        let mut ticket = ticket_requesting_web();
        ticket.model_policy = Some(ModelPolicy {
            allow_web_research: false,
            ..ModelPolicy::default()
        });
        let plan_policy = plan_policy(true);

        let recommendation = recommend_for_ticket(&ticket, Some(&plan_policy), Some("codex"))
            .expect("recommendation");

        assert!(!recommendation.web_required);
        assert!(recommendation.reason.contains("allowWebResearch=false"));
    }

    #[test]
    fn prefer_local_only_blocks_web_required_with_auditable_reason() {
        let mut ticket = ticket_requesting_web();
        ticket.model_policy = Some(ModelPolicy {
            allow_web_research: true,
            prefer_local_only: true,
            ..ModelPolicy::default()
        });

        let recommendation =
            recommend_for_ticket(&ticket, None, Some("codex")).expect("recommendation");

        assert!(!recommendation.web_required);
        assert!(
            recommendation
                .reason
                .contains("web research requested but policy blocked it")
        );
        assert!(recommendation.reason.contains("preferLocalOnly=true"));
    }
}
