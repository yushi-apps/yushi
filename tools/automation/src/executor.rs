use std::path::PathBuf;
use std::io::{BufRead, Write};
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use crate::storage::{Storage, PlanStep};
use jieyusha::messages::Message;
use jieyusha::{Registry, ToolUseContext, chat_stream_automation, AutomationContext};

/// 执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Ok,
    Error,
    Timeout,
}

/// 任务执行请求
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    /// 规则 ID（用于生成 session 目录名）
    pub rule_id: String,
    /// 规则名称
    pub rule_name: String,
    /// 规则描述
    pub rule_description: Option<String>,
    /// 要执行的消息/指令
    pub message: String,
    /// 超时秒数
    pub timeout_seconds: u32,
    /// 附加上下文（事件数据、触发时间等）
    pub context: Option<String>,
    /// 触发来源 (clock / event / manual)
    pub trigger_source: String,
}

/// 任务执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub error: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    /// 执行来源: "plan" 或 "llm"
    pub source: String,
}

/// 规则执行器
///
/// 负责在隔离会话中调用 LLM 执行任务
pub struct Executor {
    /// automation 根目录（包含 rules/、sources/、sessions/）
    automation_dir: PathBuf,
    /// 存储管理器
    storage: Storage,
}

impl Executor {
    /// 创建 Executor
    /// 
    /// - `automation_dir`: automation 根目录（包含 rules/、sources/、sessions/）
    pub fn new(automation_dir: PathBuf) -> Self {
        let storage = Storage::new(automation_dir.clone());
        Self { automation_dir, storage }
    }

    /// 执行任务：在隔离会话中调用 LLM
    pub async fn execute(&self, request: ExecutionRequest) -> ExecutionResult {
        let started_at_ms = Utc::now().timestamp_millis();
        let timeout_dur = Duration::from_secs(request.timeout_seconds as u64);
        
        // 1. 检查 plan 文件是否存在
        if let Some(steps) = self.storage.read_plan(&request.rule_id) {
            tracing::info!(
                rule_id = %request.rule_id,
                step_count = steps.len(),
                "Attempting plan replay"
            );
            
            // 2. 尝试重放 plan
            match self.replay_plan(&steps, &request.rule_id, timeout_dur).await {
                Ok(output) => {
                    // 重放成功
                    let result = ExecutionResult {
                        status: ExecutionStatus::Ok,
                        error: None,
                        started_at_ms,
                        finished_at_ms: Utc::now().timestamp_millis(),
                        source: "plan".to_string(),
                    };
                    
                    tracing::info!(
                        rule_id = %request.rule_id,
                        output_len = output.len(),
                        "Plan replay succeeded"
                    );
                    
                    self.write_run_log(&request.rule_id, &result);
                    return result;
                }
                Err(_) => {
                    // 重放失败，删除 plan 并回退到 LLM
                    tracing::warn!(
                        rule_id = %request.rule_id,
                        "Plan replay failed, clearing plan and falling back to LLM"
                    );
                    let _ = self.storage.clear_plan(&request.rule_id);
                    // 继续执行 LLM 流程
                }
            }
        }
        
        // 3. 确保共享会话目录和 workspace 存在
        // 同一规则的所有 LLM 执行共用一个目录，差量链持续追加
        let session_dir = match self.ensure_session_workspace(&request.rule_id) {
            Ok(dir) => dir,
            Err(e) => {
                return ExecutionResult {
                    status: ExecutionStatus::Error,
                    error: Some(format!("Failed to create session workspace: {}", e)),
                    started_at_ms,
                    finished_at_ms: Utc::now().timestamp_millis(),
                    source: "llm".to_string(),
                };
            }
        };
                
        // 4. 构建完整消息
        let full_message = if let Some(ctx) = &request.context {
            format!("{}\n\n## 触发上下文\n{}", request.message, ctx)
        } else {
            request.message.clone()
        };
                
        tracing::info!(
            rule_id = %request.rule_id,
            rule_name = %request.rule_name,
            trigger_source = %request.trigger_source,
            session = %session_dir.display(),
            "Starting automation task execution via LLM"
        );
        
        // 5. 构建自动化执行上下文
        let automation_ctx = AutomationContext {
            rule_name: request.rule_name.clone(),
            rule_description: request.rule_description.clone(),
            trigger_source: request.trigger_source.clone(),
        };
                
        // 6. 调用 jieyusha::chat_stream_automation() 执行 LLM 任务
        // 使用执行模式 system prompt，过滤 Rule 工具
        let mut stream = chat_stream_automation(&full_message, session_dir.clone(), automation_ctx);
        
        let collect_future = async {
            let mut response = String::new();
            while let Some(msg) = stream.next().await {
                match msg {
                    Message::Assistant(a) => {
                        response.push_str(&a.content);
                    }
                    Message::Progress(p) => {
                        tracing::debug!(
                            rule_id = %request.rule_id,
                            "progress: {}",
                            p.content.content
                        );
                    }
                    _ => {}
                }
            }
            response
        };
        
        // 7. 带超时控制消费流
        let result = match tokio::time::timeout(timeout_dur, collect_future).await {
            Ok(response) => {
                tracing::info!(
                    rule_id = %request.rule_id,
                    response_len = response.len(),
                    "Automation task completed successfully via LLM"
                );
                
                let exec_result = ExecutionResult {
                    status: ExecutionStatus::Ok,
                    error: None,
                    started_at_ms,
                    finished_at_ms: Utc::now().timestamp_millis(),
                    source: "llm".to_string(),
                };
                
                // 8. 成功后提取并保存 plan
                self.extract_and_save_plan(&session_dir, &request.rule_id);
                
                exec_result
            }
            Err(_) => {
                tracing::warn!(
                    rule_id = %request.rule_id,
                    timeout_seconds = request.timeout_seconds,
                    "Automation task timed out"
                );
                ExecutionResult {
                    status: ExecutionStatus::Timeout,
                    error: Some(format!("Task timed out after {} seconds", request.timeout_seconds)),
                    started_at_ms,
                    finished_at_ms: Utc::now().timestamp_millis(),
                    source: "llm".to_string(),
                }
            }
        };

        // 9. 写入执行日志
        self.write_run_log(&request.rule_id, &result);

        result
    }

    /// 重放 Plan：直接执行工具调用，绕过 LLM
    async fn replay_plan(
        &self,
        steps: &[PlanStep],
        rule_id: &str,
        timeout: Duration,
    ) -> Result<String, ()> {
        let mut outputs = Vec::new();
        let tools = Registry::instance().get_all_tools();
        
        let replay_future = async {
            for (idx, step) in steps.iter().enumerate() {
                // 查找工具
                let tool = match Registry::instance().get_tool(&step.tool) {
                    Some(t) => t,
                    None => {
                        tracing::error!(
                            rule_id = %rule_id,
                            tool = %step.tool,
                            "Tool not found during plan replay"
                        );
                        return Err(());
                    }
                };
                
                // 解析参数
                let input_data: serde_json::Value = match serde_json::from_str(&step.arguments) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(
                            rule_id = %rule_id,
                            tool = %step.tool,
                            error = %e,
                            "Failed to parse arguments during plan replay"
                        );
                        return Err(());
                    }
                };
                
                // 构建上下文
                let context = ToolUseContext {
                    model: None,
                    tools: tools.clone(),
                    agent_id: format!("plan-replay-{}", rule_id),
                    abort_signal: false,
                    tool_use_id: format!("plan-{}-{}", rule_id, idx),
                    root_path: None, // plan 重放不需要写 history
                };
                
                // 执行工具
                tracing::debug!(
                    rule_id = %rule_id,
                    tool = %step.tool,
                    step = idx,
                    "Executing plan step"
                );
                
                let result = tool.call(&input_data, &context).await;
                let mut stream = result.stream;
                
                // 消费结果流
                let mut step_output = String::new();
                let mut has_error = false;
                
                while let Some(msg) = stream.next().await {
                    match msg {
                        Message::Tool(tm) => {
                            step_output.push_str(&tm.content);
                            if tm.is_error {
                                has_error = true;
                                tracing::error!(
                                    rule_id = %rule_id,
                                    tool = %step.tool,
                                    step = idx,
                                    error = %tm.content,
                                    "Tool execution failed during plan replay"
                                );
                            }
                        }
                        _ => {}
                    }
                }
                
                if has_error {
                    return Err(());
                }
                
                outputs.push(step_output);
            }
            
            Ok(outputs.join("\n"))
        };
        
        // 带超时控制
        match tokio::time::timeout(timeout, replay_future).await {
            Ok(result) => result,
            Err(_) => {
                tracing::error!(
                    rule_id = %rule_id,
                    "Plan replay timed out"
                );
                Err(())
            }
        }
    }

    /// 提取并保存 Plan
    fn extract_and_save_plan(&self, session_dir: &PathBuf, rule_id: &str) {
        // 调用 get_merged_memory_xml 合并 history
        let merged_xml = match jieyusha::memory::get_merged_memory_xml(session_dir) {
            Ok(xml) => xml,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule_id,
                    error = %e,
                    "Failed to merge memory for plan extraction"
                );
                return;
            }
        };
        
        // 解析合并后的 XML，提取 status="success" 的 toolcall 节点
        let steps = self.extract_successful_toolcalls(&merged_xml);
        
        if steps.is_empty() {
            tracing::debug!(
                rule_id = %rule_id,
                "No successful toolcalls found, skipping plan save"
            );
            return;
        }
        
        // 计算相对路径
        let source_session = session_dir
            .strip_prefix(&self.automation_dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| session_dir.to_string_lossy().to_string());
        
        // 保存 plan
        if let Err(e) = self.storage.save_plan(rule_id, &steps, &source_session) {
            tracing::warn!(
                rule_id = %rule_id,
                error = %e,
                "Failed to save plan"
            );
        }
    }

    /// 从合并的 XML 中提取成功的 toolcall
    /// 使用 tuo::YNode 统一 XML 处理方式
    fn extract_successful_toolcalls(&self, xml: &str) -> Vec<PlanStep> {
        let mut steps = Vec::new();
        
        // 使用 tuo::YNode 解析 XML
        let root = match tuo::YNode::from_str(xml) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("XML parse error in plan extraction: {:?}", e);
                return steps;
            }
        };
        
        // 遍历所有子节点，找到 toolcall
        fn find_toolcalls(node: &tuo::YNode, steps: &mut Vec<PlanStep>) {
            for child in node.children() {
                if child.tag() == "toolcall" {
                    // 检查 status 属性
                    let status = child.attr("status")
                        .map(|v| v.value.to_string())
                        .unwrap_or_else(|| "success".to_string());
                    
                    if status == "success" {
                        let name = child.attr("name")
                            .map(|v| v.value.to_string())
                            .unwrap_or_default();
                        
                        let arguments = child.attr("arguments")
                            .map(|v| v.value.to_string())
                            .unwrap_or_default();
                        
                        if !name.is_empty() && !arguments.is_empty() {
                            steps.push(PlanStep {
                                tool: name,
                                arguments,
                            });
                        }
                    }
                } else {
                    // 递归查找子节点
                    find_toolcalls(child, steps);
                }
            }
        }
        
        find_toolcalls(&root, &mut steps);
        steps
    }

    /// 写入执行日志到 automation/sessions/runs/{rule_id}.jsonl
    fn write_run_log(&self, rule_id: &str, result: &ExecutionResult) {
        let runs_dir = self.automation_dir.join("sessions").join("runs");
        let _ = std::fs::create_dir_all(&runs_dir);
        let log_path = runs_dir.join(format!("{}.jsonl", rule_id));
        
        let record = serde_json::json!({
            "ts": chrono::Utc::now().timestamp_millis(),
            "rule_id": rule_id,
            "status": match result.status {
                ExecutionStatus::Ok => "ok",
                ExecutionStatus::Error => "error",
                ExecutionStatus::Timeout => "timeout",
            },
            "error": result.error,
            "source": result.source,
            "started_at_ms": result.started_at_ms,
            "finished_at_ms": result.finished_at_ms,
            "duration_ms": result.finished_at_ms - result.started_at_ms,
        });
        
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path) 
        {
            let _ = writeln!(file, "{}", record);
        }
    }

    /// 读取指定规则的最近 N 条执行日志
    pub fn read_run_logs(&self, rule_id: &str, limit: usize) -> Vec<serde_json::Value> {
        let log_path = self.automation_dir
            .join("sessions").join("runs")
            .join(format!("{}.jsonl", rule_id));
        
        // 如果文件不存在返回空 Vec
        let file = match std::fs::File::open(&log_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        
        let reader = std::io::BufReader::new(file);
        let mut logs: Vec<serde_json::Value> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();
        
        // 取最后 limit 条
        if logs.len() > limit {
            logs.drain(0..logs.len() - limit);
        }
        
        logs
    }

    /// 按 delivery 配置交付执行结果
    pub fn deliver(&self, channel: &str, target: &str, rule_id: &str, result: &ExecutionResult) {
        match channel {
            "stdout" => {
                tracing::info!(
                    rule_id = %rule_id,
                    status = ?result.status,
                    duration_ms = result.finished_at_ms - result.started_at_ms,
                    "Automation task completed"
                );
            }
            "file" => {
                if !target.is_empty() {
                    // 追加写入指定文件
                    let record = serde_json::json!({
                        "ts": chrono::Utc::now().timestamp_millis(),
                        "rule_id": rule_id,
                        "status": match result.status {
                            ExecutionStatus::Ok => "ok",
                            ExecutionStatus::Error => "error",
                            ExecutionStatus::Timeout => "timeout",
                        },
                        "error": result.error,
                        "duration_ms": result.finished_at_ms - result.started_at_ms,
                    });
                    
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(target)
                    {
                        let _ = writeln!(file, "{}", record);
                    }
                }
            }
            _ => {} // "none" 或未知，不做任何交付
        }
    }

    /// 获取 sessions 目录路径
    pub fn sessions_dir(&self) -> PathBuf {
        self.automation_dir.join("sessions")
    }

    /// 确保共享会话目录和 workspace 存在
    /// 
    /// 返回 session_dir（sessions/{rule_id}/），jieyusha 内部会追加 history/
    /// 同一规则的所有 LLM 执行共用一个目录，差量链持续追加
    fn ensure_session_workspace(&self, rule_id: &str) -> anyhow::Result<PathBuf> {
        let session_dir = self.sessions_dir().join(rule_id);
        
        // jieyusha 会在 root_path 下创建 history/ 目录
        // 所以这里只需要确保 session_dir 存在
        std::fs::create_dir_all(&session_dir)?;
        
        // 检查是否已有 workspace 文件（jieyusha 会创建）
        let workspace_path = session_dir.join("history/0_workspace.xml");
        if !workspace_path.exists() {
            // 预创建 history 目录和 workspace
            let history_dir = session_dir.join("history");
            std::fs::create_dir_all(&history_dir)?;
            self.create_automation_workspace(&history_dir)?;
            tracing::info!(
                rule_id = %rule_id,
                workspace = %workspace_path.display(),
                "Created automation session workspace"
            );
        }
        
        // 返回 session_dir，jieyusha 内部会 root_path.join("history")
        Ok(session_dir)
    }

    /// 创建自动化任务专用的精简 workspace
    /// 
    /// 包含可用的工具定义，不需要 system-prompt（使用主会话的）
    fn create_automation_workspace(&self, history_dir: &PathBuf) -> anyhow::Result<()> {
        let tools = Registry::instance().get_all_tools();
        
        let mut tools_xml = String::new();
        if !tools.is_empty() {
            tools_xml.push_str("    <available-tools>\n");
            for tool in &tools {
                let schema = tool.input_json_schema();
                // 压缩 JSON（移除多余空白）
                let compact_schema: String = schema.chars()
                    .filter(|c| !c.is_whitespace() || *c == ' ')
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<&str>>()
                    .join("");
                
                tools_xml.push_str(&format!(
                    "        <tool name=\"{}\" description=\"{}\" input-schema='{}' />\n",
                    escape_xml(tool.name()),
                    escape_xml(tool.description()),
                    escape_xml(&compact_schema)
                ));
            }
            tools_xml.push_str("    </available-tools>\n");
        }
        
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Memory id="{}" created-at="{}">
{}</Memory>
"#,
            id, created_at, tools_xml
        );
        
        let path = history_dir.join("0_workspace.xml");
        std::fs::write(&path, xml)?;
        
        Ok(())
    }

    /// 获取规则的 plan 缓存状态
    pub fn get_plan_status(&self, rule_id: &str) -> crate::storage::PlanStatus {
        self.storage.get_plan_status(rule_id)
    }

    /// 获取规则的最近执行记录
    pub fn get_last_run(&self, rule_id: &str) -> Option<LastRunInfo> {
        let logs = self.read_run_logs(rule_id, 1);
        logs.into_iter().next().map(|record| {
            let source = record.get("source").and_then(|v| v.as_str()).unwrap_or("llm").to_string();
            let status = record.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let started_at_ms = record.get("started_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let finished_at_ms = record.get("finished_at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            let duration_ms = finished_at_ms - started_at_ms;
            LastRunInfo {
                source,
                status,
                duration_ms,
                started_at_ms,
            }
        })
    }
}

/// 上次执行信息
#[derive(Debug, Clone)]
pub struct LastRunInfo {
    /// 执行来源：plan / llm
    pub source: String,
    /// 执行状态：ok / error / timeout
    pub status: String,
    /// 执行耗时（毫秒）
    pub duration_ms: i64,
    /// 执行开始时间戳
    pub started_at_ms: i64,
}

/// XML 转义特殊字符
fn escape_xml(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_successful_toolcalls() {
        // 模拟合并后的 XML（包含成功的 toolcall）
        let merged_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Memory id="test">
    <toolcall id="3" name="Skill" arguments="{&quot;name&quot;: &quot;weather&quot;}" status="success" />
    <toolcall id="5" name="Bash" arguments="{&quot;cmd&quot;: &quot;curl&quot;}" status="success" />
    <toolcall id="7" name="Bash" arguments="{&quot;cmd&quot;: &quot;ls&quot;}" status="failed" />
</Memory>
"#;
        
        let executor = Executor::new(PathBuf::from("/tmp/test"));
        let steps = executor.extract_successful_toolcalls(merged_xml);
        
        // 应该只有 2 个成功的 toolcall（排除 failed 的）
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].tool, "Skill");
        assert_eq!(steps[1].tool, "Bash");
        // 验证 XML 实体被正确解析
        assert!(steps[0].arguments.contains("weather"));
    }
}
