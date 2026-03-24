use async_trait::async_trait;
use crate::tool::{Tool, ToolUseContext, ToolResult};
use std::path::Path;
use std::fs;
use crate::utils;

pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "加载技能获取专业知识"
    }

    fn input_json_schema(&self) -> &str {
        r#"
        {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "技能名称"
                }
           },
            "required": ["name"]
        }
        "#
    }

    async fn prompt(&self) -> String {
        r#"When users ask you to perform tasks, check if any of the available skills below can help complete the task more effectively. 
        Skills provide specialized capabilities and domain knowledge."#.to_string()
    }

    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult {
        let skill_name = match input_data.get("name").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::error("Missing 'name' field in input", &context.tool_use_id),
        };

        let skill_path = Path::new("skills").join(skill_name).join("SKILL.md");
        if !skill_path.exists() {
            return ToolResult::error(format!("Skill '{}' not found", skill_name), &context.tool_use_id);
        }

        let skill_guide = match fs::read_to_string(&skill_path) {
            Ok(content) => {
                if let Ok((metadata, body)) = utils::parse_frontmatter(&content) {
                    let name = metadata.get("name").map(|s| s.as_str()).unwrap_or(skill_name);
                    let guide = body.trim();

                    format!(
                        "<skill><name>{}</name><guide>{}</guide></skill>",
                        name, guide
                    )
                } else {
                    return ToolResult::error(
                        "Failed to parse skill metadata",
                        &context.tool_use_id,
                    );
                }
            }
            Err(e) => {
                return ToolResult::error(
                    format!("Failed to read skill file: {}", e),
                    &context.tool_use_id,
                );
            }
        };

        ToolResult::result(&skill_guide, &context.tool_use_id)
    }
}

/// XML 转义
fn escape_xml(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&apos;")
}

impl SkillTool {
    pub fn load_skills(skill_dir: &Path) -> String {
        if !skill_dir.exists() {
            return String::new();
        }

        let mut skills_xml = String::from("<available-skills>\n");

        if let Ok(entries) = fs::read_dir(skill_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let skill_md_path = entry.path().join("SKILL.md");

                        if skill_md_path.exists() {
                            if let Ok(content) = fs::read_to_string(&skill_md_path) {
                                if let Ok((metadata, _)) = utils::parse_frontmatter(&content) {
                                    if let (Some(name), Some(description)) = (
                                        metadata.get("name").map(|s| s.as_str()),
                                        metadata.get("description").map(|s| s.as_str()),
                                    ) {
                                        skills_xml.push_str(&format!(
                                            "    <skill name=\"{}\" description=\"{}\" />\n",
                                            escape_xml(name), escape_xml(description)
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        skills_xml.push_str("</available-skills>");
        log::info!("Loaded skills: {}", skills_xml);
        skills_xml
    }
}