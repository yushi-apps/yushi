use std::collections::HashMap;
use crate::{JieyushaError, Result};

pub fn parse_frontmatter(content: &str) -> Result<(HashMap<String, String>, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 3 || lines[0].trim() != "---" {
        return Err(JieyushaError::ConfigError("Invalid frontmatter format: missing opening ---".to_string()));
    }

    let mut metadata_end = 0;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            metadata_end = i;
            break;
        }
    }

    if metadata_end == 0 {
        return Err(JieyushaError::ConfigError("Invalid frontmatter format: missing closing ---".to_string()));
    }

    let mut metadata = HashMap::new();
    for line in &lines[1..metadata_end] {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(colon_position) = line.find(':') {
            let (key, value) = line.split_at(colon_position);
            metadata.insert(key.trim().to_string(), value[1..].trim().to_string());
        }
    }

    let content_body = lines[metadata_end + 1..].join("\n");
    Ok((metadata, content_body))
}

