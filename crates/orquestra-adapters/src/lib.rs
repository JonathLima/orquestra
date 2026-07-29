#![allow(clippy::result_large_err)]

pub mod adapters;
pub mod capability;
pub mod install;

pub use adapters::{
    AntigravityAdapter, ClaudeCodeAdapter, CliInfo, CodexAdapter, Confidence, HostAdapter,
    OpenCodeAdapter, all_adapters, detect_all_adapters, get_adapter,
};
pub use capability::Capability;
pub use install::{FileOperation, InstallPlan, SkillRef};
