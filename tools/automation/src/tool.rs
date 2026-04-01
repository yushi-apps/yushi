//! RuleTool - Tool trait implementation for automation rule management.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use jieyusha::messages::{Message, ToolMessage};
use jieyusha::{Tool, ToolResult, ToolUseContext};

use crate::engine::RuleCommand;
use crate::rule::{Condition, ConditionGroup, MatchMode, Op, Rule, RulePatch};
use crate::storage::Storage;

/// RuleTool - manages automation rules via RuleEngine commands.
pub struct RuleTool {
    command_tx: mpsc::Sender<RuleCommand>,
    storage: Arc<Storage>,
}

impl RuleTool {
    /// Create a new RuleTool instance.
    pub fn new(command_tx: mpsc::Sender<RuleCommand>, storage: Arc<Storage>) -> Self {
        RuleTool { command_tx, storage }
    }
}

#[async_trait]
impl Tool for RuleTool {
    fn name(&self) -> &str {
        "Rule"
    }

    fn description(&self) -> &str {
        "管理自动化规则：创建、查看、更新、删除规则，支持时间驱动和事件驱动"
    }

    fn input_json_schema(&self) -> &str {
        r#"{
  "type": "object",
  "properties": {
    "action": {
      "type": "string",
      "enum": ["status", "list", "add", "update", "remove", "run", "list-sources"],
      "description": "操作类型"
    },
    "id": { "type": "string", "description": "规则 ID（update/remove/run 时必需）" },
    "name": { "type": "string", "description": "规则名称（add 时必需）" },
    "description": { "type": "string", "description": "规则描述" },
    "source_id": { "type": "string", "description": "事件源 ID，clock 表示时间驱动（add 时必需）" },
    "match": { "type": "string", "enum": ["all", "any"], "description": "顶层匹配模式，默认 all" },
    "capture_pre_seconds": { "type": "integer", "description": "捕获触发前数据秒数" },
    "conditions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "field": { "type": "string" },
          "op": { "type": "string", "enum": ["gt","lt","gte","lte","eq","ne","contains","cron-match"] },
          "value": { "type": "string" },
          "duration_seconds": { "type": "integer" }
        },
        "required": ["field", "op", "value"]
      }
    },
    "groups": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "match": { "type": "string", "enum": ["all", "any"] },
          "conditions": { "type": "array" },
          "groups": { "type": "array" }
        },
        "required": ["match"]
      }
    },
    "message": { "type": "string", "description": "触发时执行的消息（add 时必需）" },
    "timeout_seconds": { "type": "integer", "description": "超时秒数，默认 300" },
    "enabled": { "type": "boolean", "description": "是否启用，默认 true" },
    "delivery_channel": { "type": "string", "enum": ["none", "stdout", "file"], "description": "交付通道，默认 none" },
    "delivery_target": { "type": "string", "description": "交付目标（file 时为文件路径）" }
  },
  "required": ["action"]
}"#
    }

    async fn prompt(&self) -> String {
        "使用 Rule 工具管理自动化规则。支持 status/list/add/update/remove/run/list-sources 操作。".to_string()
    }

    async fn call(&self, input_data: &serde_json::Value, context: &ToolUseContext) -> ToolResult {
        let tool_use_id = context.tool_use_id.clone();
        let input = input_data.clone();
        let cmd_tx = self.command_tx.clone();
        let storage = self.storage.clone();

        let stream = async_stream::stream! {
            let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");

            match action {
                "status" => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::Status(tx)).await.is_ok() {
                        match rx.await {
                            Ok(status) => {
                                // 构建规则列表
                                let rules_json: Vec<serde_json::Value> = status.rules.iter().map(|r| {
                                    let plan_json = r.plan.as_ref().map(|p| {
                                        serde_json::json!({
                                            "exists": p.exists,
                                            "path": p.path,
                                            "created_at_ms": p.created_at_ms,
                                            "steps_count": p.steps_count,
                                        })
                                    });
                                    
                                    let last_run_json = r.last_run.as_ref().map(|lr| {
                                        serde_json::json!({
                                            "source": lr.source,
                                            "status": lr.status,
                                            "duration_ms": lr.duration_ms,
                                        })
                                    });
                                    
                                    serde_json::json!({
                                        "id": r.id,
                                        "name": r.name,
                                        "enabled": r.enabled,
                                        "plan": plan_json,
                                        "last_run": last_run_json,
                                    })
                                }).collect();
                                
                                let result = serde_json::json!({
                                    "total_rules": status.total_rules,
                                    "enabled_rules": status.enabled_rules,
                                    "next_clock_wake_ms": status.next_clock_wake_ms,
                                    "active_sources": status.active_sources,
                                    "rules": rules_json,
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to get status: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        yield Message::Tool(ToolMessage::from_error(
                            "Failed to send status command", &tool_use_id
                        ));
                    }
                }

                "list" => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::List(tx)).await.is_ok() {
                        match rx.await {
                            Ok(rules) => {
                                let rules_json: Vec<serde_json::Value> = rules.iter().map(|r| {
                                    serde_json::json!({
                                        "id": r.id,
                                        "name": r.name,
                                        "description": r.description,
                                        "enabled": r.enabled,
                                        "source_id": r.source_id,
                                        "message": r.message,
                                        "timeout_seconds": r.timeout_seconds,
                                    })
                                }).collect();
                                let result = serde_json::json!({
                                    "count": rules.len(),
                                    "rules": rules_json
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to list rules: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        yield Message::Tool(ToolMessage::from_error(
                            "Failed to send list command", &tool_use_id
                        ));
                    }
                }

                "add" => {
                    match parse_rule_from_json(&input) {
                        Ok(rule) => {
                            let rule_id = rule.id.clone();

                            // WAL: 先写差量文件，再修改内存
                            if let Err(e) = storage.create_rule_add_delta(&rule) {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to persist rule: {}", e), &tool_use_id
                                ));
                                return;
                            }

                            // 差量文件写入成功，发送命令给 Engine
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            if cmd_tx.send(RuleCommand::Add(rule, tx)).await.is_ok() {
                                match rx.await {
                                    Ok(Ok(())) => {
                                        let result = serde_json::json!({
                                            "success": true,
                                            "id": rule_id,
                                            "message": "规则创建成功"
                                        });
                                        yield Message::Tool(ToolMessage::new_content(
                                            &result.to_string(), &tool_use_id
                                        ));
                                    }
                                    Ok(Err(e)) => {
                                        // 内存修改失败，但差量文件已写入
                                        // 重启时会从差量文件恢复，数据最终一致
                                        yield Message::Tool(ToolMessage::from_error(
                                            &format!("Rule persisted but engine error: {}", e), &tool_use_id
                                        ));
                                    }
                                    Err(e) => {
                                        yield Message::Tool(ToolMessage::from_error(
                                            &format!("Rule persisted but channel error: {}", e), &tool_use_id
                                        ));
                                    }
                                }
                            } else {
                                // 发送命令失败，但差量文件已写入
                                yield Message::Tool(ToolMessage::from_error(
                                    "Rule persisted but failed to send command", &tool_use_id
                                ));
                            }
                        }
                        Err(e) => {
                            yield Message::Tool(ToolMessage::from_error(
                                &format!("Invalid rule parameters: {}", e), &tool_use_id
                            ));
                        }
                    }
                }

                "update" => {
                    let id = match input.get("id").and_then(|v| v.as_str()) {
                        Some(id) => id.to_string(),
                        None => {
                            yield Message::Tool(ToolMessage::from_error(
                                "Missing required parameter: id", &tool_use_id
                            ));
                            return;
                        }
                    };

                    let patch = parse_patch_from_json(&input);
                    let should_clear_plan = patch.message.is_some();

                    // WAL: 先写差量文件，再修改内存
                    if let Err(e) = storage.create_rule_update_delta_by_id(&id, &patch) {
                        yield Message::Tool(ToolMessage::from_error(
                            &format!("Failed to persist rule update: {}", e), &tool_use_id
                        ));
                        return;
                    }

                    // 差量文件写入成功，发送命令给 Engine
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::Update(id.clone(), patch, tx)).await.is_ok() {
                        match rx.await {
                            Ok(Ok(())) => {
                                // 如果 message 发生了变更，清空对应的 plan 缓存
                                if should_clear_plan {
                                    if let Err(e) = storage.clear_plan(&id) {
                                        tracing::warn!(rule_id = %id, error = %e, "Failed to clear plan cache");
                                        // 不阻断 update 流程，仅警告
                                    }
                                }
                                let result = serde_json::json!({
                                    "success": true,
                                    "id": id,
                                    "message": "规则更新成功"
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Ok(Err(e)) => {
                                // 内存修改失败，但差量文件已写入
                                // 重启时会从差量文件恢复，数据最终一致
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Rule update persisted but engine error: {}", e), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Rule update persisted but channel error: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        // 发送命令失败，但差量文件已写入
                        yield Message::Tool(ToolMessage::from_error(
                            "Rule update persisted but failed to send command", &tool_use_id
                        ));
                    }
                }

                "remove" => {
                    let id = match input.get("id").and_then(|v| v.as_str()) {
                        Some(id) => id.to_string(),
                        None => {
                            yield Message::Tool(ToolMessage::from_error(
                                "Missing required parameter: id", &tool_use_id
                            ));
                            return;
                        }
                    };

                    // WAL: 先写差量文件，再修改内存
                    if let Err(e) = storage.create_rule_remove_delta(&id) {
                        yield Message::Tool(ToolMessage::from_error(
                            &format!("Failed to persist rule removal: {}", e), &tool_use_id
                        ));
                        return;
                    }

                    // 差量文件写入成功，发送命令给 Engine
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::Remove(id.clone(), tx)).await.is_ok() {
                        match rx.await {
                            Ok(Ok(())) => {
                                let result = serde_json::json!({
                                    "success": true,
                                    "id": id,
                                    "message": "规则删除成功"
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Ok(Err(e)) => {
                                // 内存修改失败，但差量文件已写入
                                // 重启时会从差量文件恢复，数据最终一致
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Rule removal persisted but engine error: {}", e), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Rule removal persisted but channel error: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        // 发送命令失败，但差量文件已写入
                        yield Message::Tool(ToolMessage::from_error(
                            "Rule removal persisted but failed to send command", &tool_use_id
                        ));
                    }
                }

                "run" => {
                    let id = match input.get("id").and_then(|v| v.as_str()) {
                        Some(id) => id.to_string(),
                        None => {
                            yield Message::Tool(ToolMessage::from_error(
                                "Missing required parameter: id", &tool_use_id
                            ));
                            return;
                        }
                    };

                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::RunNow(id.clone(), tx)).await.is_ok() {
                        match rx.await {
                            Ok(Ok(())) => {
                                let result = serde_json::json!({
                                    "success": true,
                                    "id": id,
                                    "message": "规则已触发执行"
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Ok(Err(e)) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to run rule: {}", e), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to run rule: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        yield Message::Tool(ToolMessage::from_error(
                            "Failed to send run command", &tool_use_id
                        ));
                    }
                }

                "list-sources" => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if cmd_tx.send(RuleCommand::ListSources(tx)).await.is_ok() {
                        match rx.await {
                            Ok(sources) => {
                                let sources_json: Vec<serde_json::Value> = sources.iter().map(|s| {
                                    serde_json::json!({
                                        "id": s.id,
                                        "name": s.name,
                                        "type": format!("{:?}", s.source_type).to_lowercase(),
                                        "endpoint": s.endpoint,
                                        "topic": s.topic,
                                    })
                                }).collect();
                                let result = serde_json::json!({
                                    "count": sources.len(),
                                    "sources": sources_json
                                });
                                yield Message::Tool(ToolMessage::new_content(
                                    &result.to_string(), &tool_use_id
                                ));
                            }
                            Err(e) => {
                                yield Message::Tool(ToolMessage::from_error(
                                    &format!("Failed to list sources: {}", e), &tool_use_id
                                ));
                            }
                        }
                    } else {
                        yield Message::Tool(ToolMessage::from_error(
                            "Failed to send list-sources command", &tool_use_id
                        ));
                    }
                }

                _ => {
                    yield Message::Tool(ToolMessage::from_error(
                        &format!("Unknown action: {}", action), &tool_use_id
                    ));
                }
            }
        };

        ToolResult::new(Box::pin(stream))
    }
}

/// Parse a Rule from JSON input.
fn parse_rule_from_json(input: &serde_json::Value) -> anyhow::Result<Rule> {
    let name = input.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?
        .to_string();

    let source_id = input.get("source_id")
        .and_then(|v| v.as_str())
        .unwrap_or("clock")
        .to_string();

    let message = input.get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?
        .to_string();

    let id = uuid::Uuid::new_v4().to_string();

    let mut rule = Rule::new(id, name, source_id, message);

    if let Some(desc) = input.get("description").and_then(|v| v.as_str()) {
        rule.description = Some(desc.to_string());
    }

    if let Some(enabled) = input.get("enabled").and_then(|v| v.as_bool()) {
        rule.enabled = enabled;
    }

    if let Some(match_mode) = input.get("match").and_then(|v| v.as_str()) {
        rule.match_mode = MatchMode::from_str(match_mode).unwrap_or(MatchMode::All);
    }

    if let Some(capture) = input.get("capture_pre_seconds").and_then(|v| v.as_u64()) {
        rule.capture_pre_seconds = Some(capture as u32);
    }

    if let Some(timeout) = input.get("timeout_seconds").and_then(|v| v.as_u64()) {
        rule.timeout_seconds = timeout as u32;
    }

    if let Some(channel) = input.get("delivery_channel").and_then(|v| v.as_str()) {
        rule.delivery_channel = channel.to_string();
    }

    if let Some(target) = input.get("delivery_target").and_then(|v| v.as_str()) {
        rule.delivery_target = target.to_string();
    }

    if let Some(conditions) = input.get("conditions").and_then(|v| v.as_array()) {
        rule.conditions = parse_conditions_from_json(conditions);
    }

    if let Some(groups) = input.get("groups").and_then(|v| v.as_array()) {
        rule.groups = parse_groups_from_json(groups);
    }

    Ok(rule)
}

/// Parse conditions from JSON array.
fn parse_conditions_from_json(arr: &[serde_json::Value]) -> Vec<Condition> {
    arr.iter().filter_map(|v| {
        let field = v.get("field").and_then(|f| f.as_str())?.to_string();
        let op_str = v.get("op").and_then(|o| o.as_str())?;
        let op = Op::from_str(op_str)?;
        let value = v.get("value").and_then(|val| val.as_str())?.to_string();
        let duration_seconds = v.get("duration_seconds").and_then(|d| d.as_u64()).map(|d| d as u32);

        Some(Condition {
            field,
            op,
            value,
            duration_seconds,
        })
    }).collect()
}

/// Parse condition groups from JSON array.
fn parse_groups_from_json(arr: &[serde_json::Value]) -> Vec<ConditionGroup> {
    arr.iter().filter_map(|v| {
        let match_str = v.get("match").and_then(|m| m.as_str()).unwrap_or("all");
        let match_mode = MatchMode::from_str(match_str).unwrap_or(MatchMode::All);

        let conditions = v.get("conditions")
            .and_then(|c| c.as_array())
            .map(|c| parse_conditions_from_json(c))
            .unwrap_or_default();

        let groups = v.get("groups")
            .and_then(|g| g.as_array())
            .map(|g| parse_groups_from_json(g))
            .unwrap_or_default();

        Some(ConditionGroup {
            match_mode,
            conditions,
            groups,
        })
    }).collect()
}

/// Parse a RulePatch from JSON input.
fn parse_patch_from_json(input: &serde_json::Value) -> RulePatch {
    let mut patch = RulePatch::default();

    if let Some(name) = input.get("name").and_then(|v| v.as_str()) {
        patch.name = Some(name.to_string());
    }

    if let Some(desc) = input.get("description") {
        if desc.is_null() {
            patch.description = Some(None);
        } else if let Some(s) = desc.as_str() {
            patch.description = Some(Some(s.to_string()));
        }
    }

    if let Some(enabled) = input.get("enabled").and_then(|v| v.as_bool()) {
        patch.enabled = Some(enabled);
    }

    if let Some(message) = input.get("message").and_then(|v| v.as_str()) {
        patch.message = Some(message.to_string());
    }

    if let Some(timeout) = input.get("timeout_seconds").and_then(|v| v.as_u64()) {
        patch.timeout_seconds = Some(timeout as u32);
    }

    if let Some(channel) = input.get("delivery_channel").and_then(|v| v.as_str()) {
        patch.delivery_channel = Some(channel.to_string());
    }

    if let Some(target) = input.get("delivery_target").and_then(|v| v.as_str()) {
        patch.delivery_target = Some(target.to_string());
    }

    if let Some(conditions) = input.get("conditions").and_then(|v| v.as_array()) {
        patch.conditions = Some(parse_conditions_from_json(conditions));
    }

    if let Some(groups) = input.get("groups").and_then(|v| v.as_array()) {
        patch.groups = Some(parse_groups_from_json(groups));
    }

    patch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rule_from_json() {
        let input = serde_json::json!({
            "name": "测试规则",
            "source_id": "clock",
            "message": "执行任务",
            "conditions": [
                {
                    "field": "time",
                    "op": "cron-match",
                    "value": "0 8 * * *"
                }
            ]
        });

        let rule = parse_rule_from_json(&input).unwrap();
        assert_eq!(rule.name, "测试规则");
        assert_eq!(rule.source_id, "clock");
        assert_eq!(rule.message, "执行任务");
        assert_eq!(rule.conditions.len(), 1);
        assert_eq!(rule.conditions[0].op, Op::CronMatch);
    }

    #[test]
    fn test_parse_patch_from_json() {
        let input = serde_json::json!({
            "id": "rule_001",
            "name": "新名称",
            "enabled": false
        });

        let patch = parse_patch_from_json(&input);
        assert_eq!(patch.name, Some("新名称".to_string()));
        assert_eq!(patch.enabled, Some(false));
        assert!(patch.message.is_none());
    }

    #[test]
    fn test_parse_conditions() {
        let conditions = vec![
            serde_json::json!({
                "field": "temperature",
                "op": "gt",
                "value": "30",
                "duration_seconds": 60
            }),
            serde_json::json!({
                "field": "humidity",
                "op": "lt",
                "value": "20"
            })
        ];

        let parsed = parse_conditions_from_json(&conditions);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].field, "temperature");
        assert_eq!(parsed[0].op, Op::Gt);
        assert_eq!(parsed[0].duration_seconds, Some(60));
        assert_eq!(parsed[1].field, "humidity");
        assert_eq!(parsed[1].op, Op::Lt);
        assert!(parsed[1].duration_seconds.is_none());
    }

    #[test]
    fn test_parse_groups() {
        let groups = vec![
            serde_json::json!({
                "match": "any",
                "conditions": [
                    { "field": "a", "op": "eq", "value": "1" }
                ],
                "groups": []
            })
        ];

        let parsed = parse_groups_from_json(&groups);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].match_mode, MatchMode::Any);
        assert_eq!(parsed[0].conditions.len(), 1);
    }
}
