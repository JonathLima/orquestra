use petgraph::graph::DiGraph;
use std::collections::{HashMap, VecDeque};

use crate::error::PlanError;
use crate::types::*;

pub fn derive_waves(plan: &Plan) -> Result<WaveResult, PlanError> {
    use crate::validate::validate_plan;
    let validation = validate_plan(plan);
    if !validation.valid {
        let details: Vec<String> = validation
            .errors
            .iter()
            .map(|e| format!("[{}] {}", e.code, e.message))
            .collect();
        return Err(PlanError::Invalid(format!(
            "plan has validation errors: {}",
            details.join("; ")
        )));
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

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut wave_of: HashMap<&str, u32> = HashMap::new();
    let mut wave_tickets: HashMap<u32, Vec<String>> = HashMap::new();

    for ticket in &plan.tickets {
        in_degree.insert(ticket.id.as_str(), ticket.blocked_by.len());
    }

    let mut queue: VecDeque<&str> = VecDeque::new();
    for ticket in &plan.tickets {
        if ticket.blocked_by.is_empty() {
            queue.push_back(ticket.id.as_str());
            wave_of.insert(ticket.id.as_str(), 1);
        }
    }

    let mut processed = 0;
    while let Some(tid) = queue.pop_front() {
        processed += 1;
        let w = wave_of[tid];
        wave_tickets.entry(w).or_default().push(tid.to_string());

        if let Some(&idx) = node_indices.get(tid) {
            for neighbor in graph.neighbors(idx) {
                let nid = graph[neighbor];
                *in_degree.get_mut(nid).expect("node missing from in_degree") -= 1;
                let new_wave = w + 1;
                let current = wave_of.entry(nid).or_insert(new_wave);
                if new_wave > *current {
                    *current = new_wave;
                }
                if in_degree[nid] == 0 {
                    queue.push_back(nid);
                }
            }
        }
    }

    let mut waves: Vec<Wave> = wave_tickets
        .into_iter()
        .map(|(num, ids)| Wave {
            wave_number: num,
            ticket_ids: ids,
        })
        .collect();
    waves.sort_by_key(|w| w.wave_number);

    Ok(WaveResult {
        total_waves: waves.len() as u32,
        total_tickets: processed,
        waves,
    })
}
