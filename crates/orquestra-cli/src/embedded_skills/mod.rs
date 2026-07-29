use orquestra_adapters::install::SkillRef;

pub const ORCHESTRATOR_SKILL: &str = include_str!("orquestra-orchestrator/SKILL.md");
pub const PLANNER_SKILL: &str = include_str!("orquestra-planner/SKILL.md");
pub const ROUTER_SKILL: &str = include_str!("orquestra-router/SKILL.md");
pub const VERIFIER_SKILL: &str = include_str!("orquestra-verifier/SKILL.md");
pub const INIT_SKILL: &str = include_str!("orquestra-init/SKILL.md");
pub const GRILL_SKILL: &str = include_str!("orquestra-grill/SKILL.md");
pub const GRILL_WITH_DOCS_SKILL: &str = include_str!("orquestra-grill-with-docs/SKILL.md");

pub fn all_skills() -> Vec<SkillRef> {
    vec![
        SkillRef {
            name: "orquestra-orchestrator".to_string(),
            content: ORCHESTRATOR_SKILL,
        },
        SkillRef {
            name: "orquestra-planner".to_string(),
            content: PLANNER_SKILL,
        },
        SkillRef {
            name: "orquestra-router".to_string(),
            content: ROUTER_SKILL,
        },
        SkillRef {
            name: "orquestra-verifier".to_string(),
            content: VERIFIER_SKILL,
        },
        SkillRef {
            name: "orquestra-init".to_string(),
            content: INIT_SKILL,
        },
        SkillRef {
            name: "orquestra-grill".to_string(),
            content: GRILL_SKILL,
        },
        SkillRef {
            name: "orquestra-grill-with-docs".to_string(),
            content: GRILL_WITH_DOCS_SKILL,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_skills_are_not_empty() {
        let skills = all_skills();
        assert_eq!(skills.len(), 7);
        for skill in &skills {
            assert!(!skill.content.is_empty(), "{} is empty", skill.name);
            assert!(
                skill.content.starts_with("---") || skill.content.trim_start().starts_with('#'),
                "{} should start with frontmatter or heading",
                skill.name
            );
        }
    }

    #[test]
    fn test_official_grill_skills_are_embedded_and_match_package_copies() {
        let skills = all_skills();
        let package_skills_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/skills");

        for embedded in &skills {
            let packaged =
                fs::read_to_string(package_skills_dir.join(&embedded.name).join("SKILL.md"))
                    .unwrap_or_else(|error| {
                        panic!("missing package copy for {}: {error}", embedded.name)
                    });

            assert_eq!(
                embedded.content.replace("\r\n", "\n"),
                packaged.replace("\r\n", "\n"),
                "{} package copy drifted",
                embedded.name
            );
        }
    }

    #[test]
    fn test_all_skills_have_frontmatter() {
        for skill in all_skills() {
            assert!(
                skill.content.starts_with("---"),
                "{} missing YAML frontmatter",
                skill.name
            );
            assert!(
                skill.content.contains("name:"),
                "{} missing name in frontmatter",
                skill.name
            );
            assert!(
                skill.content.contains("description:"),
                "{} missing description in frontmatter",
                skill.name
            );
        }
    }

    #[test]
    fn test_normal_discovery_flow_selects_the_grill_engine_from_project_context() {
        for (name, flow) in [
            ("orquestra-orchestrator", ORCHESTRATOR_SKILL),
            ("orquestra-init", INIT_SKILL),
        ] {
            assert!(
                flow.contains("Select `orquestra-grill` for a new project or when no document content is authorized."),
                "{name} must select the standard grill engine"
            );
            assert!(
                flow.contains("Select `orquestra-grill-with-docs` for an existing project."),
                "{name} must select the document-aware grill engine"
            );
            assert!(
                flow.contains("Obtain consolidated consent for candidate paths and file types before reading any document content."),
                "{name} must enforce document consent before reads"
            );
            assert!(
                !flow.contains("Generate 1 question per relevant category")
                    && !flow.contains("Questions to ask (when needed):"),
                "{name} must delegate adaptive questions instead of using fixed categories"
            );
        }
    }

    #[test]
    fn test_discovery_skills_use_current_confidence_and_research_contract() {
        for (name, flow) in [
            ("orquestra-orchestrator", ORCHESTRATOR_SKILL),
            ("orquestra-init", INIT_SKILL),
        ] {
            for obsolete in [
                "4-source",
                "≥4",
                "confidence < 0.92",
                "Score ≥ 7.0",
                "max 3 loops",
                "max loops (3)",
                "force user override",
            ] {
                assert!(
                    !flow.contains(obsolete),
                    "{name} still advertises obsolete contract: {obsolete}"
                );
            }
            assert!(
                flow.contains("0.95"),
                "{name} must use the default confidence gate"
            );
            assert!(
                flow.contains("5-source") || flow.contains("five sources"),
                "{name} must require five-source cross-validation"
            );
        }
    }

    #[test]
    fn test_public_research_skills_do_not_advertise_four_source_rule() {
        for (name, content) in [
            ("embedded orchestrator", ORCHESTRATOR_SKILL),
            (
                "package orchestrator",
                include_str!("../../../../packages/cli/skills/orquestra-orchestrator/SKILL.md"),
            ),
        ] {
            assert!(
                !content.contains("4-Source Rule") && !content.contains("--max-sources 4"),
                "{name} advertises the obsolete four-source contract"
            );
        }
    }
}
