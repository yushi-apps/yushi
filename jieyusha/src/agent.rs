use std::fs;
use std::path::Path;
use std::collections::HashMap;

use glob::glob;

use crate::error::{JieyushaError, Result};

#[derive(Clone)]
pub struct AgentConfig {
    pub agent_type: String,
    pub description: String,
    pub model_name: String,
    pub tools: Vec<String>,
    pub system_prompt: String,
}

fn parse_frontmatter(content: &str) -> Result<(HashMap<String, String>, String)> {
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

fn parse_tools(tools_str: &str) -> Vec<String> {
    log::info!("Parsing tools: {}", tools_str);
    tools_str.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect()
}

pub fn scan_agent_directory(agent_path: &str) -> Result<Vec<AgentConfig>> {
    let mut agent_configs = Vec::new();
    let dir_path = Path::new(agent_path);
    if !dir_path.exists() {
        return Err(JieyushaError::ConfigError(format!("Agent directory does not exist: {}", agent_path)));
    }

    if !dir_path.is_dir() {
        return Err(JieyushaError::ConfigError(format!("Agent path is not a directory: {}", agent_path)));
    }

    let pattern = format!("{}/**/*.md", agent_path);
    for entry in glob(&pattern).map_err(|e| JieyushaError::ConfigError(format!("Glob error: {}", e)))? {
        match entry {
            Ok(path) => {
                let content = fs::read_to_string(&path)
                .map_err(|e| JieyushaError::ConfigError(format!("Failed to read file: {:?}: {}", path, e)))?;

                match parse_frontmatter(&content) {
                    Ok((metadata, body)) => {
                        let agent_type = metadata.get("name").cloned().unwrap_or_default();
                        let description = metadata.get("description").cloned().unwrap_or_default();
                        let model_name = metadata.get("model").cloned().unwrap_or_default();
                        let tools = metadata.get("tools")
                            .map(|tools_str| parse_tools(tools_str))
                            .unwrap_or_default();

                        let config = AgentConfig {
                            agent_type,
                            description,
                            model_name,
                            tools,
                            system_prompt: body.trim().to_string(),
                        };

                        agent_configs.push(config);
                    }
                    Err(e) => {
                        log::warn!("Parsing frontmatter in {:?}: {}", path, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                log::warn!("Reading path: {}", e);
            }
        }
    }

    Ok(agent_configs)
}

pub fn parse_agent_config(content: &str) -> Option<AgentConfig> { 
    match parse_frontmatter(content) {
        Ok((metadata, body)) => {
            let agent_type = metadata.get("name").cloned().unwrap_or_default();
            let description = metadata.get("description").cloned().unwrap_or_default();
            let model_name = metadata.get("model").cloned().unwrap_or_default();

            let tools = metadata.get("tools")
                .map(|tools_str| parse_tools(tools_str))
                .unwrap_or_default();

            log::info!("Agent tools: {:?}", tools);
            Some(AgentConfig {
                agent_type,
                description,
                model_name,
                tools,
                system_prompt: body.trim().to_string(),
            })
        }
        Err(e) => {
            log::warn!("Parsing frontmatter failed: {}", e);
            None
        }
    }
}