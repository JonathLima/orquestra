use orquestra_core::error::OrquestraError;
use serde::Deserialize;

fn string_or_seq<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct StringOrSeq;
    impl<'de> de::Visitor<'de> for StringOrSeq {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("string or sequence of strings")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Vec<String>, A::Error> {
            Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }
    deserializer.deserialize_any(StringOrSeq)
}

#[derive(Debug, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default, deserialize_with = "string_or_seq")]
    pub compatibility: Vec<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default, deserialize_with = "string_or_seq")]
    pub capabilities: Vec<String>,
}

pub fn parse_frontmatter(content: &str) -> Result<Option<SkillFrontmatter>, OrquestraError> {
    let content = content.trim_start_matches(|c: char| c == '\u{FEFF}' || c.is_whitespace());
    if !content.starts_with("---") {
        return Ok(None);
    }
    let end = content[3..].find("---");
    match end {
        None => Ok(None),
        Some(end_pos) => {
            let yaml_str = &content[3..3 + end_pos];
            let fm: SkillFrontmatter = serde_yaml::from_str(yaml_str)
                .map_err(|e| OrquestraError::from(format!("Invalid frontmatter: {e}")))?;
            Ok(Some(fm))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
version: 1.0.0
---"#;
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.version, Some("1.0.0".to_string()));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Just a heading\n\nSome content";
        let result = parse_frontmatter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_malformed_yaml() {
        let content = "---\nname: [broken\n---";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_minimal_frontmatter() {
        let content = "---\nname: minimal\n---";
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.name, "minimal");
        assert!(fm.capabilities.is_empty());
    }
}
