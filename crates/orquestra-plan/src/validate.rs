use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use petgraph::visit::Dfs;
use std::collections::{HashMap, HashSet};

use crate::types::*;

pub fn validate_plan(plan: &Plan) -> ValidationResult {
    let mut errors = Vec::new();
    let warnings = Vec::new(); // ponytail: warnings deferred, all violations are errors for now

    if plan.schema_version != 1 {
        errors.push(ValidationError {
            code: "unsupported_schema".to_string(),
            message: format!("Unsupported schema version: {}", plan.schema_version),
            ticket_id: None,
        });
    }

    if plan.title.trim().is_empty() {
        errors.push(ValidationError {
            code: "missing_title".to_string(),
            message: "Plan title is required".to_string(),
            ticket_id: None,
        });
    }

    if plan.tickets.is_empty() {
        errors.push(ValidationError {
            code: "no_tickets".to_string(),
            message: "Plan must have at least one ticket".to_string(),
            ticket_id: None,
        });
        return ValidationResult {
            valid: false,
            errors,
            warnings,
        };
    }

    let ids: HashSet<&str> = plan.tickets.iter().map(|t| t.id.as_str()).collect();

    let mut seen = HashSet::new();
    for ticket in &plan.tickets {
        if ticket.id.trim().is_empty() {
            errors.push(ValidationError {
                code: "empty_id".to_string(),
                message: "Ticket has an empty ID".to_string(),
                ticket_id: None,
            });
        }
        if !seen.insert(ticket.id.as_str()) {
            errors.push(ValidationError {
                code: "duplicate_id".to_string(),
                message: format!("Duplicate ticket ID: {}", ticket.id),
                ticket_id: Some(ticket.id.clone()),
            });
        }
    }

    for ticket in &plan.tickets {
        if ticket.objective.trim().is_empty() {
            errors.push(ValidationError {
                code: "missing_objective".to_string(),
                message: format!("Ticket {} has no objective", ticket.id),
                ticket_id: Some(ticket.id.clone()),
            });
        }
        if ticket.acceptance_criteria.is_empty() {
            errors.push(ValidationError {
                code: "missing_criteria".to_string(),
                message: format!("Ticket {} has no acceptance criteria", ticket.id),
                ticket_id: Some(ticket.id.clone()),
            });
        }
        if ticket
            .assigned_skill
            .as_deref()
            .is_none_or(|skill| skill.trim().is_empty())
        {
            errors.push(ValidationError {
                code: "missing_assigned_skill".to_string(),
                message: format!("Ticket {} has no assigned skill", ticket.id),
                ticket_id: Some(ticket.id.clone()),
            });
        }
        if ticket.verification.minimum_score < 0.0 || ticket.verification.minimum_score > 1.0 {
            errors.push(ValidationError {
                code: "invalid_score".to_string(),
                message: format!(
                    "Ticket {} score {} out of range [0.0, 1.0]",
                    ticket.id, ticket.verification.minimum_score
                ),
                ticket_id: Some(ticket.id.clone()),
            });
        }
        for dep in &ticket.blocked_by {
            if !ids.contains(dep.as_str()) {
                errors.push(ValidationError {
                    code: "missing_dependency".to_string(),
                    message: format!("Ticket {} depends on unknown ticket: {}", ticket.id, dep),
                    ticket_id: Some(ticket.id.clone()),
                });
            }
        }
    }

    let mut graph = DiGraph::<&str, ()>::new();
    let mut node_indices = HashMap::new();
    for ticket in &plan.tickets {
        let idx = graph.add_node(ticket.id.as_str());
        node_indices.insert(ticket.id.as_str(), idx);
    }
    for ticket in &plan.tickets {
        if let Some(dependent) = node_indices.get(ticket.id.as_str()) {
            for dep in &ticket.blocked_by {
                if let Some(dependency) = node_indices.get(dep.as_str()) {
                    graph.add_edge(*dependency, *dependent, ());
                }
            }
        }
    }

    match toposort(&graph, None) {
        Ok(_) => {}
        Err(cycle) => {
            let node = graph[cycle.node_id()];
            errors.push(ValidationError {
                code: "cycle_detected".to_string(),
                message: format!("Dependency cycle detected involving ticket: {node}"),
                ticket_id: Some(node.to_string()),
            });
        }
    }

    let roots: Vec<_> = plan
        .tickets
        .iter()
        .filter(|t| t.blocked_by.is_empty())
        .map(|t| t.id.as_str())
        .collect();

    if !roots.is_empty() {
        let mut visited = HashSet::new();
        for root in &roots {
            if let Some(start) = node_indices.get(root) {
                let mut dfs = Dfs::new(&graph, *start);
                while let Some(nx) = dfs.next(&graph) {
                    visited.insert(graph[nx]);
                }
            }
        }
        for ticket in &plan.tickets {
            if !visited.contains(ticket.id.as_str()) {
                errors.push(ValidationError {
                    code: "unreachable_ticket".to_string(),
                    message: format!("Ticket {} is unreachable from any root", ticket.id),
                    ticket_id: Some(ticket.id.clone()),
                });
            }
        }
    } else if plan.tickets.iter().any(|t| !t.blocked_by.is_empty()) {
        errors.push(ValidationError {
            code: "no_roots".to_string(),
            message: "No root tickets found (all tickets have dependencies)".to_string(),
            ticket_id: None,
        });
    }

    let valid = errors.is_empty();
    ValidationResult {
        valid,
        errors,
        warnings,
    }
}
