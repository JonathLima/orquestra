use crate::types::*;

pub fn explain_plan(plan: &Plan, waves: &WaveResult, validation: &ValidationResult) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Plan: {}\n\n", plan.title));
    out.push_str(&format!("- Total tickets: {}\n", waves.total_tickets));
    out.push_str(&format!("- Total waves:   {}\n\n", waves.total_waves));

    out.push_str("## Validation\n\n");
    if validation.valid {
        out.push_str("Status: **valid**\n\n");
    } else {
        out.push_str(&format!(
            "Status: **invalid** ({} errors)\n\n",
            validation.errors.len()
        ));
        for err in &validation.errors {
            match &err.ticket_id {
                Some(tid) => out.push_str(&format!("- [{}] {}: {}\n", err.code, tid, err.message)),
                None => out.push_str(&format!("- [{}] {}\n", err.code, err.message)),
            }
        }
        out.push('\n');
    }

    if !validation.warnings.is_empty() {
        out.push_str("Warnings:\n");
        for w in &validation.warnings {
            out.push_str(&format!("- {w}\n"));
        }
        out.push('\n');
    }

    if validation.valid {
        out.push_str("## Waves\n\n");
        for wave in &waves.waves {
            out.push_str(&format!("### Wave {}\n\n", wave.wave_number));
            for tid in &wave.ticket_ids {
                let ticket = plan.tickets.iter().find(|t| t.id == *tid);
                match ticket {
                    Some(t) => {
                        out.push_str(&format!("- **{}**: {} ({})\n", t.id, t.title, t.objective))
                    }
                    None => out.push_str(&format!("- {tid}\n")),
                }
            }
            out.push('\n');
        }

        let critical_path = waves.waves.len();
        out.push_str(&format!(
            "### Critical path length: {} waves\n",
            critical_path
        ));
    }

    out
}
