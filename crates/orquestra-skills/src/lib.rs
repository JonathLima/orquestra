#![allow(clippy::result_large_err)]

pub mod brain;
pub mod frontmatter;
pub mod inventory;
pub mod matching;
pub mod scanner;
pub mod types;

pub use brain::{
    BrainCandidate, BrainCandidateStatus, BrainPolicy, adapt_local_skill, approve_candidate,
    brain_policy, external_discovery_disabled, inspect_candidate, reject_candidate,
};
pub use frontmatter::{SkillFrontmatter, parse_frontmatter};
pub use inventory::{
    inventory_md_path, inventory_path, read_inventory, render_markdown, write_inventory,
};
pub use matching::{SkillMatch, SkillMatchReport, match_plan, match_ticket};
pub use scanner::{
    SkillScannerConfig, compute_hash, default_scan_sources, scan_all, scan_skill_dir,
};
pub use types::{Provenance, ScanSource, SkillInfo, SkillInventory, SkillStatus, TrustLevel};
