//! Persistent storage module for automation rules and event sources.
//!
//! This module implements delta-chain based storage using tuo's xdsl merge capabilities.

use std::path::{Path, PathBuf};

use crate::rule::{Rule, Rules, RulePatch};
use crate::source::{EventSource, EventSources};
use tuo::ynode::YNode;
use tuo::xdsl::YNodeMerge;

/// Plan 中的单个步骤
#[derive(Debug, Clone)]
pub struct PlanStep {
    pub tool: String,
    pub arguments: String,
}

/// Plan 缓存状态（元信息）
#[derive(Debug, Clone)]
pub struct PlanStatus {
    pub exists: bool,
    pub path: String,
    pub created_at_ms: Option<i64>,
    pub steps_count: usize,
}

/// Persistent storage manager for automation configuration.
pub struct Storage {
    /// automation root directory
    automation_dir: PathBuf,
}

impl Storage {
    /// Create a new storage instance.
    pub fn new(automation_dir: PathBuf) -> Self {
        Storage { automation_dir }
    }

    /// Rules delta directory.
    pub fn rules_dir(&self) -> PathBuf {
        self.automation_dir.join("rules")
    }

    /// Event sources delta directory.
    pub fn sources_dir(&self) -> PathBuf {
        self.automation_dir.join("sources")
    }

    /// Plans cache directory.
    fn plans_dir(&self) -> PathBuf {
        self.automation_dir.join("plans")
    }

    /// Ensure directory structure exists.
    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.rules_dir())?;
        std::fs::create_dir_all(self.sources_dir())?;
        std::fs::create_dir_all(self.automation_dir.join("sessions"))?;
        Ok(())
    }

    /// Initialize base files (called on first startup).
    pub fn init_base_files(&self) -> anyhow::Result<()> {
        // sources/0_base.xml - contains built-in clock
        let sources_base = self.sources_dir().join("0_base.xml");
        if !sources_base.exists() {
            let sources = EventSources::default_with_clock();
            std::fs::write(&sources_base, sources.to_xml())?;
        }

        // rules/0_base.xml - empty rule set
        let rules_base = self.rules_dir().join("0_base.xml");
        if !rules_base.exists() {
            let rules = Rules { rules: vec![] };
            std::fs::write(&rules_base, rules.to_xml())?;
        }

        Ok(())
    }

    /// Get next sequence number in directory.
    fn next_sequence_number(dir: &Path) -> u32 {
        Self::scan_xml_files(dir)
            .iter()
            .filter_map(|name| {
                // File name format: {N}_{action}_{id}.xml
                name.split('_').next()?.parse::<u32>().ok()
            })
            .max()
            .map(|n| n + 1)
            .unwrap_or(1)
    }

    /// Get last XML file name in directory (for x:extends).
    fn last_file_name(dir: &Path) -> Option<String> {
        let mut files = Self::scan_xml_files(dir);
        
        // Sort by sequence number
        files.sort_by(|a, b| {
            let seq_a = a.split('_').next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            let seq_b = b.split('_').next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            seq_a.cmp(&seq_b)
        });
        
        files.last().cloned()
    }

    /// Get last XML file path in directory.
    fn last_xml_file_path(dir: &Path) -> Option<PathBuf> {
        Self::last_file_name(dir).map(|name| dir.join(name))
    }

    /// Scan XML files in directory.
    fn scan_xml_files(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.ends_with(".xml") && !name.ends_with(".xml.tmp") {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Create rule add delta file.
    pub fn create_rule_add_delta(&self, rule: &Rule) -> anyhow::Result<()> {
        let dir = self.rules_dir();
        let seq = Self::next_sequence_number(&dir);
        let extends = Self::last_file_name(&dir)
            .unwrap_or_else(|| "0_base.xml".to_string());

        let rules = Rules { rules: vec![rule.clone()] };
        let xml = rules.to_delta_xml(&extends);

        let filename = format!("{}_add_{}.xml", seq, rule.id);
        self.atomic_write(&dir.join(&filename), &xml)?;
        Ok(())
    }

    /// Create rule update delta file.
    pub fn create_rule_update_delta(&self, rule: &Rule, patch: &RulePatch) -> anyhow::Result<()> {
        self.create_rule_update_delta_by_id(&rule.id, patch)
    }

    /// Create rule update delta file by rule_id (for WAL pattern).
    pub fn create_rule_update_delta_by_id(&self, rule_id: &str, patch: &RulePatch) -> anyhow::Result<()> {
        let dir = self.rules_dir();
        let seq = Self::next_sequence_number(&dir);
        let extends = Self::last_file_name(&dir)
            .unwrap_or_else(|| "0_base.xml".to_string());

        // Generate XML with only modified attributes, with x:override="merge"
        let xml_element = crate::rule::rule_merge_xml_element(rule_id, patch);
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Rules xmlns:x=\"/nop/schema/xdsl.xdef\" x:extends=\"{}\">\n{}\n</Rules>",
            extends, xml_element
        );

        let filename = format!("{}_update_{}.xml", seq, rule_id);
        self.atomic_write(&dir.join(&filename), &xml)?;
        Ok(())
    }

    /// Create rule remove delta file.
    pub fn create_rule_remove_delta(&self, rule_id: &str) -> anyhow::Result<()> {
        let dir = self.rules_dir();
        let seq = Self::next_sequence_number(&dir);
        let extends = Self::last_file_name(&dir)
            .unwrap_or_else(|| "0_base.xml".to_string());

        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Rules xmlns:x=\"/nop/schema/xdsl.xdef\" x:extends=\"{}\">\n    <rule id=\"{}\" x:override=\"remove\" />\n</Rules>",
            extends, rule_id
        );

        let filename = format!("{}_remove_{}.xml", seq, rule_id);
        self.atomic_write(&dir.join(&filename), &xml)?;
        Ok(())
    }


    /// Create event source add delta file.
    pub fn create_source_add_delta(&self, source: &EventSource) -> anyhow::Result<()> {
        let dir = self.sources_dir();
        let seq = Self::next_sequence_number(&dir);
        let extends = Self::last_file_name(&dir)
            .unwrap_or_else(|| "0_base.xml".to_string());

        let sources = EventSources { sources: vec![source.clone()] };
        let xml = sources.to_delta_xml(&extends);

        let filename = format!("{}_add_{}.xml", seq, source.id);
        self.atomic_write(&dir.join(&filename), &xml)?;
        Ok(())
    }

    /// Load and merge all rule deltas to get current rule set.
    pub fn load_merged_rules(&self) -> anyhow::Result<Vec<Rule>> {
        let dir = self.rules_dir();
        let last_file = Self::last_xml_file_path(&dir);

        match last_file {
            Some(path) => {
                // Use tuo's process_extends to recursively merge
                let node = YNode::from_xml(path.to_str().unwrap())?;
                let merged = node.process_extends(&dir)?;
                let xml = merged.to_xml();
                let rules = Rules::from_xml(&xml)?;
                Ok(rules.rules)
            }
            None => Ok(vec![]),
        }
    }

    /// Load and merge all event source deltas.
    pub fn load_merged_sources(&self) -> anyhow::Result<Vec<EventSource>> {
        let dir = self.sources_dir();
        let last_file = Self::last_xml_file_path(&dir);

        match last_file {
            Some(path) => {
                // Use tuo's process_extends to recursively merge
                let node = YNode::from_xml(path.to_str().unwrap())?;
                let merged = node.process_extends(&dir)?;
                let xml = merged.to_xml();
                let sources = EventSources::from_xml(&xml)?;
                Ok(sources.sources)
            }
            None => Ok(vec![]),
        }
    }

    /// Atomic file write (tmp → fsync → rename).
    fn atomic_write(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        use std::io::Write;

        let tmp_path = path.with_extension("xml.tmp");
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// 清空指定规则的 plan 缓存文件
    pub fn clear_plan(&self, rule_id: &str) -> anyhow::Result<bool> {
        let plan_path = self.plans_dir().join(format!("{}.task.xml", rule_id));
        if plan_path.exists() {
            std::fs::remove_file(&plan_path)?;
            tracing::info!(rule_id = %rule_id, "Plan cache cleared");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 获取 plan 缓存的元信息状态
    pub fn get_plan_status(&self, rule_id: &str) -> PlanStatus {
        let plan_path = self.plans_dir().join(format!("{}.task.xml", rule_id));
        let relative_path = format!("plans/{}.task.xml", rule_id);
        
        if !plan_path.exists() {
            return PlanStatus {
                exists: false,
                path: relative_path,
                created_at_ms: None,
                steps_count: 0,
            };
        }

        let content = match std::fs::read_to_string(&plan_path) {
            Ok(c) => c,
            Err(_) => return PlanStatus {
                exists: false,
                path: relative_path,
                created_at_ms: None,
                steps_count: 0,
            },
        };

        // 使用 quick-xml 解析 task 元素的 created-at-ms 属性和 step 数量
        let mut created_at_ms: Option<i64> = None;
        let mut steps_count = 0;
        let mut reader = quick_xml::Reader::from_str(&content);
        reader.trim_text(true);
        let mut buf = Vec::new();
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                    if tag == "task" {
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            if key == "created-at-ms" {
                                created_at_ms = val.parse::<i64>().ok();
                            }
                        }
                    }
                }
                Ok(quick_xml::events::Event::Empty(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                    if tag == "step" {
                        steps_count += 1;
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        PlanStatus {
            exists: true,
            path: relative_path,
            created_at_ms,
            steps_count,
        }
    }

    /// 读取 plan 文件，解析为步骤列表
    /// 支持两种格式：
    /// - 新格式：<step tool="..."><arguments>...</arguments></step>
    /// - 旧格式：<step tool="..." arguments="..." />（兼容）
    pub fn read_plan(&self, rule_id: &str) -> Option<Vec<PlanStep>> {
        let plan_path = self.plans_dir().join(format!("{}.task.xml", rule_id));
        if !plan_path.exists() {
            return None;
        }

        let content = match std::fs::read_to_string(&plan_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(rule_id = %rule_id, error = %e, "Failed to read plan file");
                return None;
            }
        };

        // 使用 quick-xml 解析 step 节点
        let mut steps = Vec::new();
        let mut reader = quick_xml::Reader::from_str(&content);
        reader.trim_text(true);
        let mut buf = Vec::new();
        
        // 当前正在解析的 step
        let mut current_step: Option<(String, Option<String>)> = None;
        let mut in_arguments = false;
        let mut arguments_content = String::new();
        
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                    if tag == "step" {
                        // 开始解析 step，提取 tool 属性
                        let mut tool: Option<String> = None;
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            let val = unescape_xml_attr(val);
                            if key == "tool" {
                                tool = Some(val);
                            }
                        }
                        if let Some(t) = tool {
                            current_step = Some((t, None));
                        }
                    } else if tag == "arguments" && current_step.is_some() {
                        // 进入 arguments 子元素
                        in_arguments = true;
                        arguments_content.clear();
                    }
                }
                Ok(quick_xml::events::Event::Empty(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                    if tag == "step" {
                        // 旧格式：<step tool="..." arguments="..." />
                        let mut tool: Option<String> = None;
                        let mut arguments: Option<String> = None;
                        
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            let val = unescape_xml_attr(val);
                            match key {
                                "tool" => tool = Some(val),
                                "arguments" => arguments = Some(val),
                                _ => {}
                            }
                        }
                        
                        if let (Some(t), Some(a)) = (tool, arguments) {
                            steps.push(PlanStep { tool: t, arguments: a });
                        }
                    }
                }
                Ok(quick_xml::events::Event::Text(ref e)) => {
                    // 读取 arguments 的文本内容
                    if in_arguments {
                        if let Ok(text) = e.unescape() {
                            arguments_content.push_str(&text);
                        }
                    }
                }
                Ok(quick_xml::events::Event::End(ref e)) => {
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                    if tag == "arguments" && in_arguments {
                        // 结束 arguments，保存内容
                        in_arguments = false;
                        if let Some((tool, _)) = &mut current_step {
                            current_step = Some((tool.clone(), Some(arguments_content.clone())));
                        }
                    } else if tag == "step" {
                        // 结束 step，添加到结果
                        if let Some((tool, args)) = current_step.take() {
                            if let Some(arguments) = args {
                                steps.push(PlanStep { tool, arguments });
                            }
                        }
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => {
                    tracing::warn!(rule_id = %rule_id, error = %e, "XML parse error in plan file");
                    break;
                }
                _ => {}
            }
            buf.clear();
        }

        if steps.is_empty() {
            tracing::warn!(rule_id = %rule_id, "Plan file contains no steps");
            None
        } else {
            tracing::info!(rule_id = %rule_id, step_count = steps.len(), "Loaded plan from cache");
            Some(steps)
        }
    }

    /// 保存 plan 文件
    pub fn save_plan(&self, rule_id: &str, steps: &[PlanStep], source_session: &str) -> anyhow::Result<()> {
        let plans_dir = self.plans_dir();
        std::fs::create_dir_all(&plans_dir)?;

        let plan_path = plans_dir.join(format!("{}.task.xml", rule_id));
        let created_at_ms = chrono::Utc::now().timestamp_millis();

        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push('\n');
        xml.push_str(&format!(
            r#"<task xmlns:x="/nop/schema/xdsl.xdef"{}      source-session="{}"{}      created-at-ms="{}">"#,
            "\n", source_session, "\n", created_at_ms
        ));
        xml.push('\n');

        for step in steps {
            // 使用子元素格式，避免 JSON 双引号转义问题
            // arguments 作为元素内容，需要转义 < > & 但不需要转义双引号
            xml.push_str(&format!(
                "    <step tool=\"{}\">\n        <arguments>{}</arguments>\n    </step>\n",
                escape_xml_attr(&step.tool),
                escape_xml_content(&step.arguments)
            ));
        }

        xml.push_str("</task>\n");

        self.atomic_write(&plan_path, &xml)?;
        tracing::info!(
            rule_id = %rule_id,
            step_count = steps.len(),
            source_session = %source_session,
            "Plan saved to cache"
        );

        Ok(())
    }
}

/// XML 属性转义（转义所有特殊字符）
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// XML 元素内容转义（只转义 < > &，双引号和单引号无需转义）
fn escape_xml_content(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// XML 属性反转义
fn unescape_xml_attr(s: &str) -> String {
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_dirs() {
        let dir = PathBuf::from("/tmp/automation");
        let storage = Storage::new(dir.clone());
        
        assert_eq!(storage.rules_dir(), dir.join("rules"));
        assert_eq!(storage.sources_dir(), dir.join("sources"));
    }

    #[test]
    fn test_next_sequence_number_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let seq = Storage::next_sequence_number(temp_dir.path());
        assert_eq!(seq, 1);
    }

    #[test]
    fn test_scan_xml_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("0_base.xml"), "<Rules/>").unwrap();
        std::fs::write(temp_dir.path().join("1_add_test.xml"), "<Rules/>").unwrap();
        std::fs::write(temp_dir.path().join("ignore.txt"), "not xml").unwrap();
        
        let files = Storage::scan_xml_files(temp_dir.path());
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"0_base.xml".to_string()));
        assert!(files.contains(&"1_add_test.xml".to_string()));
    }

    #[test]
    fn test_last_file_name() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("0_base.xml"), "<Rules/>").unwrap();
        std::fs::write(temp_dir.path().join("1_add_r1.xml"), "<Rules/>").unwrap();
        std::fs::write(temp_dir.path().join("2_update_r1.xml"), "<Rules/>").unwrap();
        
        let last = Storage::last_file_name(temp_dir.path());
        assert_eq!(last, Some("2_update_r1.xml".to_string()));
    }

    #[test]
    fn test_clear_plan_existing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path().to_path_buf());
        
        // 创建 plans 目录和 plan 文件
        let plans_dir = temp_dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        let plan_path = plans_dir.join("rule_001.task.xml");
        std::fs::write(&plan_path, "<task/>").unwrap();
        
        // 确认文件存在
        assert!(plan_path.exists());
        
        // 清空 plan
        let result = storage.clear_plan("rule_001").unwrap();
        assert!(result); // 返回 true 表示文件被删除
        assert!(!plan_path.exists()); // 文件应该被删除
    }

    #[test]
    fn test_clear_plan_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path().to_path_buf());
        
        // 创建 plans 目录但不创建 plan 文件
        let plans_dir = temp_dir.path().join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        
        // 清空不存在的 plan，应该返回 false 而不报错
        let result = storage.clear_plan("nonexistent_rule").unwrap();
        assert!(!result); // 返回 false 表示文件不存在
    }

    #[test]
    fn test_save_and_read_plan() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path().to_path_buf());
        
        // 创建测试 steps
        let steps = vec![
            PlanStep {
                tool: "Bash".to_string(),
                arguments: r#"{"command":"curl -s wttr.in/?format=%t"}"#.to_string(),
            },
            PlanStep {
                tool: "FileRead".to_string(),
                arguments: r#"{"path":"/tmp/test.txt"}"#.to_string(),
            },
        ];
        
        // 保存 plan
        storage.save_plan("rule_001", &steps, "sessions/rule_001_123").unwrap();
        
        // 读取 plan
        let loaded = storage.read_plan("rule_001").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].tool, "Bash");
        assert!(loaded[0].arguments.contains("curl"));
        assert_eq!(loaded[1].tool, "FileRead");
    }

    #[test]
    fn test_read_plan_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path().to_path_buf());
        
        // 读取不存在的 plan
        let result = storage.read_plan("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_plan_xml_escape() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path().to_path_buf());
        
        // 测试包含特殊字符的 arguments
        let steps = vec![
            PlanStep {
                tool: "Bash".to_string(),
                arguments: r#"{"cmd":"echo '<test>' && echo \"done\""}"#.to_string(),
            },
        ];
        
        storage.save_plan("rule_special", &steps, "sessions/test").unwrap();
        
        let loaded = storage.read_plan("rule_special").unwrap();
        assert_eq!(loaded.len(), 1);
        // 确认特殊字符被正确转义和反转义
        assert!(loaded[0].arguments.contains("<test>"));
        assert!(loaded[0].arguments.contains("\""));
    }
}
