use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Capability {
    Subagents,
    NonInteractive,
    Hooks,
    InstructionsOnly,
    FileSystem,
    WebSearch,
}

impl Capability {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Subagents => "Can spawn sub-agents for parallel execution",
            Self::NonInteractive => "Supports headless/non-interactive execution",
            Self::Hooks => "Supports lifecycle hooks",
            Self::InstructionsOnly => "Accepts text-based instructions only",
            Self::FileSystem => "Can read and write files",
            Self::WebSearch => "Can search the web via MCP/WIE",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Subagents => write!(f, "subagents"),
            Self::NonInteractive => write!(f, "non-interactive"),
            Self::Hooks => write!(f, "hooks"),
            Self::InstructionsOnly => write!(f, "instructions-only"),
            Self::FileSystem => write!(f, "filesystem"),
            Self::WebSearch => write!(f, "web-search"),
        }
    }
}
