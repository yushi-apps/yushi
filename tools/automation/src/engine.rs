//! RuleEngine - 统一规则引擎
//!
//! 负责调度时钟规则和事件规则的评估与执行。

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use chrono::Utc;
use croner::Cron;
use tokio::sync::{mpsc, oneshot};

use crate::executor::{ExecutionRequest, Executor};
use crate::rule::{Condition, ConditionGroup, MatchMode, Op, Rule, RulePatch};
use crate::source::{EventData, EventSource};


/// 规则引擎命令
pub enum RuleCommand {
    Add(Rule, oneshot::Sender<anyhow::Result<()>>),
    Update(String, RulePatch, oneshot::Sender<anyhow::Result<()>>),
    Remove(String, oneshot::Sender<anyhow::Result<()>>),
    RunNow(String, oneshot::Sender<anyhow::Result<()>>),
    List(oneshot::Sender<Vec<Rule>>),
    ListSources(oneshot::Sender<Vec<EventSource>>),
    Status(oneshot::Sender<EngineStatus>),
    Shutdown,
}

/// 引擎状态
pub struct EngineStatus {
    pub total_rules: usize,
    pub enabled_rules: usize,
    pub next_clock_wake_ms: Option<i64>,
    pub active_sources: usize,
    /// 每个规则的详细状态
    pub rules: Vec<RuleStatusInfo>,
}

/// 单个规则的状态信息
#[derive(Debug, Clone)]
pub struct RuleStatusInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Plan 缓存状态
    pub plan: Option<PlanCacheStatus>,
    /// 上次执行状态
    pub last_run: Option<LastRunStatus>,
}

/// Plan 缓存状态
#[derive(Debug, Clone)]
pub struct PlanCacheStatus {
    pub exists: bool,
    pub path: String,
    pub created_at_ms: Option<i64>,
    pub steps_count: usize,
}

/// 上次执行状态
#[derive(Debug, Clone)]
pub struct LastRunStatus {
    /// 执行来源：plan / llm
    pub source: String,
    /// 执行状态：ok / error / timeout
    pub status: String,
    /// 执行耗时（毫秒）
    pub duration_ms: i64,
    /// 执行开始时间戳
    pub started_at_ms: i64,
}

/// 时钟规则状态
struct ClockRuleState {
    rule_id: String,
    next_match_at_ms: i64,
}

/// 规则评估状态（用于跟踪条件持续时间）
struct RuleEvalState {
    rule_id: String,
    /// 每个条件的持续满足开始时间
    condition_start_times: Vec<Option<i64>>,
    /// 上次触发时间（防抖）
    last_triggered_at: Option<i64>,
}

/// 环形缓冲区，用于存储事件数据
struct RingBuffer {
    data: VecDeque<EventData>,
    max_duration_ms: i64,
}

impl RingBuffer {
    fn new(max_duration_ms: i64) -> Self {
        Self {
            data: VecDeque::new(),
            max_duration_ms,
        }
    }

    fn push(&mut self, event: EventData) {
        let now_ms = Utc::now().timestamp_millis();
        let cutoff = now_ms - self.max_duration_ms;

        // 移除过期数据
        while let Some(front) = self.data.front() {
            if front.timestamp_ms < cutoff {
                self.data.pop_front();
            } else {
                break;
            }
        }

        self.data.push_back(event);
    }

    fn get_range(&self, from_ms: i64, to_ms: i64) -> Vec<&EventData> {
        self.data
            .iter()
            .filter(|e| e.timestamp_ms >= from_ms && e.timestamp_ms <= to_ms)
            .collect()
    }
}

/// RuleEngine 核心
pub struct RuleEngine {
    rules: HashMap<String, Rule>,
    sources: HashMap<String, EventSource>,
    executor: Arc<Executor>,
    clock_states: Vec<ClockRuleState>,
    eval_states: HashMap<String, RuleEvalState>,
    ring_buffers: HashMap<String, RingBuffer>,
    command_rx: mpsc::Receiver<RuleCommand>,
    event_rx: mpsc::Receiver<EventData>,
    /// 正在执行的规则 ID 集合（用于并发保护）
    running_rules: HashSet<String>,
}

impl RuleEngine {
    /// 启动规则引擎，返回命令发送端和事件发送端
    pub fn start(
        executor: Arc<Executor>,
        rules: Vec<Rule>,
        sources: Vec<EventSource>,
    ) -> (
        mpsc::Sender<RuleCommand>,
        mpsc::Sender<EventData>,
        tokio::task::JoinHandle<()>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(256);

        let rules_map: HashMap<String, Rule> = rules.into_iter().map(|r| (r.id.clone(), r)).collect();
        let sources_map: HashMap<String, EventSource> =
            sources.into_iter().map(|s| (s.id.clone(), s)).collect();

        // 初始化评估状态
        let eval_states: HashMap<String, RuleEvalState> = rules_map
            .iter()
            .map(|(id, rule)| {
                let total_conditions = Self::count_conditions(rule);
                (
                    id.clone(),
                    RuleEvalState {
                        rule_id: id.clone(),
                        condition_start_times: vec![None; total_conditions],
                        last_triggered_at: None,
                    },
                )
            })
            .collect();

        // 初始化环形缓冲区（默认 1 小时）
        let ring_buffers: HashMap<String, RingBuffer> = sources_map
            .keys()
            .map(|id| (id.clone(), RingBuffer::new(3600 * 1000)))
            .collect();

        let mut engine = RuleEngine {
            rules: rules_map,
            sources: sources_map,
            executor,
            clock_states: Vec::new(),
            eval_states,
            ring_buffers,
            command_rx: cmd_rx,
            event_rx,
            running_rules: HashSet::new(),
        };

        engine.init_clock_states();

        let handle = tokio::spawn(async move {
            engine.run().await;
        });

        (cmd_tx, event_tx, handle)
    }

    /// 统计规则中的条件总数
    fn count_conditions(rule: &Rule) -> usize {
        let mut count = rule.conditions.len();
        for group in &rule.groups {
            count += Self::count_group_conditions(group);
        }
        count
    }

    fn count_group_conditions(group: &ConditionGroup) -> usize {
        let mut count = group.conditions.len();
        for sub_group in &group.groups {
            count += Self::count_group_conditions(sub_group);
        }
        count
    }

    /// 初始化时钟规则状态
    fn init_clock_states(&mut self) {
        self.clock_states.clear();

        for (id, rule) in &self.rules {
            if !rule.enabled || !rule.is_clock_rule() {
                continue;
            }

            // 找到 cron-match 条件
            if let Some(cron_expr) = Self::find_cron_expression(rule) {
                if let Some(next_ms) = Self::compute_next_cron_match(&cron_expr) {
                    self.clock_states.push(ClockRuleState {
                        rule_id: id.clone(),
                        next_match_at_ms: next_ms,
                    });
                }
            }
        }
    }

    /// 在规则中查找 cron 表达式
    fn find_cron_expression(rule: &Rule) -> Option<String> {
        // 先检查顶层条件
        for cond in &rule.conditions {
            if cond.op == Op::CronMatch {
                return Some(cond.value.clone());
            }
        }
        // 再检查组内条件
        for group in &rule.groups {
            if let Some(expr) = Self::find_cron_in_group(group) {
                return Some(expr);
            }
        }
        None
    }

    fn find_cron_in_group(group: &ConditionGroup) -> Option<String> {
        for cond in &group.conditions {
            if cond.op == Op::CronMatch {
                return Some(cond.value.clone());
            }
        }
        for sub_group in &group.groups {
            if let Some(expr) = Self::find_cron_in_group(sub_group) {
                return Some(expr);
            }
        }
        None
    }

    /// 计算 cron 表达式的下次匹配时间
    fn compute_next_cron_match(cron_expr: &str) -> Option<i64> {
        let cron = Cron::new(cron_expr).parse().ok()?;
        let next = cron.find_next_occurrence(&Utc::now(), false).ok()?;
        Some(next.timestamp_millis())
    }

    /// 计算下次时钟唤醒时间
    fn compute_next_clock_wake(&self) -> Option<i64> {
        self.clock_states
            .iter()
            .map(|s| s.next_match_at_ms)
            .min()
    }

    /// 核心运行循环
    async fn run(&mut self) {
        loop {
            let next_wake = self.compute_next_clock_wake();

            tokio::select! {
                // 分支1：时钟规则到期
                _ = Self::sleep_until_opt(next_wake) => {
                    self.execute_due_clock_rules().await;
                }
                // 分支2：收到外部事件
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event).await;
                }
                // 分支3：收到管理命令
                Some(cmd) = self.command_rx.recv() => {
                    if self.handle_command(cmd).await {
                        break; // Shutdown
                    }
                }
            }
        }
    }

    /// 可选的 sleep：如果没有时钟规则则永远挂起
    async fn sleep_until_opt(wake_ms: Option<i64>) {
        match wake_ms {
            Some(ms) => {
                let now = Utc::now().timestamp_millis();
                let delay = (ms - now).max(0) as u64;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
            None => {
                // 没有时钟规则时永远不触发此分支
                std::future::pending::<()>().await;
            }
        }
    }

    /// 执行到期的时钟规则
    async fn execute_due_clock_rules(&mut self) {
        let now = Utc::now().timestamp_millis();

        // 收集到期的规则 ID
        let due_rule_ids: Vec<String> = self
            .clock_states
            .iter()
            .filter(|s| s.next_match_at_ms <= now)
            .map(|s| s.rule_id.clone())
            .collect();

        for rule_id in due_rule_ids {
            // 并发保护：如果规则正在执行，跳过本次触发
            if self.running_rules.contains(&rule_id) {
                tracing::debug!(rule_id = %rule_id, "Skipping clock rule: already running");
                continue;
            }

            if let Some(rule) = self.rules.get(&rule_id).cloned() {
                if !rule.enabled {
                    continue;
                }

                // 标记为正在执行
                self.running_rules.insert(rule_id.clone());

                // 构造执行请求
                let request = ExecutionRequest {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    rule_description: rule.description.clone(),
                    message: rule.message.clone(),
                    timeout_seconds: rule.timeout_seconds,
                    context: Some(format!("时钟触发时间: {}", Utc::now().to_rfc3339())),
                    trigger_source: "clock".to_string(),
                };

                // 执行
                let result = self.executor.execute(request).await;

                // 交付结果
                self.executor.deliver(
                    &rule.delivery_channel,
                    &rule.delivery_target,
                    &rule_id,
                    &result,
                );

                // 移除正在执行标记
                self.running_rules.remove(&rule_id);

                // 重新计算下次触发时间
                if let Some(cron_expr) = Self::find_cron_expression(&rule) {
                    if let Some(next_ms) = Self::compute_next_cron_match(&cron_expr) {
                        if let Some(state) = self.clock_states.iter_mut().find(|s| s.rule_id == rule_id) {
                            state.next_match_at_ms = next_ms;
                        }
                    }
                }
            }
        }
    }

    /// 处理事件
    async fn handle_event(&mut self, event: EventData) {
        let source_id = event.source_id.clone();
        let timestamp_ms = event.timestamp_ms;

        // 1. 存入环形缓冲区
        if let Some(buffer) = self.ring_buffers.get_mut(&source_id) {
            buffer.push(event.clone());
        }

        // 2. 遍历 source_id 匹配的规则
        let matching_rule_ids: Vec<String> = self
            .rules
            .iter()
            .filter(|(_, r)| r.enabled && r.source_id == source_id)
            .map(|(id, _)| id.clone())
            .collect();

        for rule_id in matching_rule_ids {
            let rule = match self.rules.get(&rule_id) {
                Some(r) => r.clone(),
                None => continue,
            };

            // 3. 评估条件
            let triggered = self.evaluate_rule(&rule, &event, timestamp_ms);

            if triggered {
                // 并发保护：如果规则正在执行，跳过本次触发
                if self.running_rules.contains(&rule_id) {
                    tracing::debug!(rule_id = %rule_id, "Skipping event rule: already running");
                    continue;
                }

                // 标记为正在执行
                self.running_rules.insert(rule_id.clone());

                // 4. 构造执行请求并执行
                let context = serde_json::to_string_pretty(&event.fields).ok();
                let request = ExecutionRequest {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    rule_description: rule.description.clone(),
                    message: rule.message.clone(),
                    timeout_seconds: rule.timeout_seconds,
                    context,
                    trigger_source: "event".to_string(),
                };

                let result = self.executor.execute(request).await;

                // 交付结果
                self.executor.deliver(
                    &rule.delivery_channel,
                    &rule.delivery_target,
                    &rule_id,
                    &result,
                );

                // 移除正在执行标记
                self.running_rules.remove(&rule_id);

                // 更新 last_triggered_at
                if let Some(state) = self.eval_states.get_mut(&rule_id) {
                    state.last_triggered_at = Some(timestamp_ms);
                }
            }
        }
    }

    /// 评估规则
    fn evaluate_rule(&mut self, rule: &Rule, event: &EventData, now_ms: i64) -> bool {
        // 评估顶层条件
        let conditions_result = self.evaluate_conditions_with_duration(
            &rule.conditions,
            event,
            now_ms,
            &rule.id,
            0,
            rule.match_mode.clone(),
        );

        // 计算顶层 groups 的起始索引（顶层条件之后）
        let groups_start_idx = rule.conditions.len();
        
        // 评估顶层组
        let groups_result = if rule.groups.is_empty() {
            true
        } else {
            let mut idx = groups_start_idx;
            let mut results = Vec::new();
            for group in &rule.groups {
                let result = self.evaluate_group(group, event, now_ms, &rule.id, idx);
                results.push(result);
                // 更新下一个 group 的起始索引
                idx += Self::count_group_conditions(group);
            }
            match rule.match_mode {
                MatchMode::All => results.iter().all(|&r| r),
                MatchMode::Any => results.iter().any(|&r| r),
            }
        };

        // 根据 match_mode 组合结果
        match rule.match_mode {
            MatchMode::All => conditions_result && groups_result,
            MatchMode::Any => {
                if rule.conditions.is_empty() && rule.groups.is_empty() {
                    true
                } else {
                    conditions_result || groups_result
                }
            }
        }
    }

    /// 评估带持续时间的条件列表
    fn evaluate_conditions_with_duration(
        &mut self,
        conditions: &[Condition],
        event: &EventData,
        now_ms: i64,
        rule_id: &str,
        start_idx: usize,
        match_mode: MatchMode,
    ) -> bool {
        if conditions.is_empty() {
            return true;
        }

        let results: Vec<bool> = conditions
            .iter()
            .enumerate()
            .map(|(i, cond)| {
                let immediate_result = self.evaluate_condition(cond, event);
                let idx = start_idx + i;

                // 处理 duration_seconds
                if let Some(duration_sec) = cond.duration_seconds {
                    let duration_ms = duration_sec as i64 * 1000;

                    if let Some(state) = self.eval_states.get_mut(rule_id) {
                        if idx < state.condition_start_times.len() {
                            if immediate_result {
                                // 条件满足，记录或检查开始时间
                                match state.condition_start_times[idx] {
                                    Some(start_time) => {
                                        // 检查是否已持续足够时间
                                        now_ms - start_time >= duration_ms
                                    }
                                    None => {
                                        // 首次满足，记录开始时间
                                        state.condition_start_times[idx] = Some(now_ms);
                                        false
                                    }
                                }
                            } else {
                                // 条件不满足，重置开始时间
                                state.condition_start_times[idx] = None;
                                false
                            }
                        } else {
                            immediate_result
                        }
                    } else {
                        immediate_result
                    }
                } else {
                    immediate_result
                }
            })
            .collect();

        match match_mode {
            MatchMode::All => results.iter().all(|&r| r),
            MatchMode::Any => results.iter().any(|&r| r),
        }
    }

    /// 单个条件评估（不含 duration 逻辑）
    fn evaluate_condition(&self, cond: &Condition, event: &EventData) -> bool {
        match cond.op {
            Op::CronMatch => {
                // 检查当前时间是否匹配 cron 表达式
                if let Ok(cron) = Cron::new(&cond.value).parse() {
                    let now = Utc::now();
                    // 检查当前时间是否在 cron 的匹配范围内（1分钟窗口）
                    if let Ok(next) = cron.find_next_occurrence(&now, true) {
                        let diff = (next.timestamp() - now.timestamp()).abs();
                        diff < 60 // 1分钟窗口
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => {
                // 获取字段值
                let field_value = match event.fields.get(&cond.field) {
                    Some(v) => v,
                    None => return false,
                };

                // 尝试数值比较
                let field_num = Self::value_to_f64(field_value);
                let cond_num = cond.value.parse::<f64>().ok();

                match cond.op {
                    Op::Gt => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            f > c
                        } else {
                            false
                        }
                    }
                    Op::Lt => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            f < c
                        } else {
                            false
                        }
                    }
                    Op::Gte => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            f >= c
                        } else {
                            false
                        }
                    }
                    Op::Lte => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            f <= c
                        } else {
                            false
                        }
                    }
                    Op::Eq => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            (f - c).abs() < f64::EPSILON
                        } else {
                            // 字符串比较
                            Self::value_to_string(field_value) == cond.value
                        }
                    }
                    Op::Ne => {
                        if let (Some(f), Some(c)) = (field_num, cond_num) {
                            (f - c).abs() >= f64::EPSILON
                        } else {
                            // 字符串比较
                            Self::value_to_string(field_value) != cond.value
                        }
                    }
                    Op::Contains => {
                        let field_str = Self::value_to_string(field_value);
                        field_str.contains(&cond.value)
                    }
                    Op::CronMatch => unreachable!(),
                }
            }
        }
    }

    /// 将 serde_json::Value 转换为 f64
    fn value_to_f64(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// 将 serde_json::Value 转换为 String
    fn value_to_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "null".to_string(),
            _ => value.to_string(),
        }
    }

    /// 递归评估条件组
    fn evaluate_group(&mut self, group: &ConditionGroup, event: &EventData, now_ms: i64, rule_id: &str, start_idx: usize) -> bool {
        // 评估组内条件（支持 duration）
        let conditions_result = if group.conditions.is_empty() {
            true
        } else {
            self.evaluate_conditions_with_duration(
                &group.conditions,
                event,
                now_ms,
                rule_id,
                start_idx,
                group.match_mode.clone(),
            )
        };

        // 计算嵌套组的起始索引
        let nested_start_idx = start_idx + group.conditions.len();
        
        // 评估嵌套组
        let groups_result = if group.groups.is_empty() {
            true
        } else {
            let mut idx = nested_start_idx;
            let mut results = Vec::new();
            for sub_group in &group.groups {
                let result = self.evaluate_group(sub_group, event, now_ms, rule_id, idx);
                results.push(result);
                // 更新下一个 group 的起始索引
                idx += Self::count_group_conditions(sub_group);
            }
            match group.match_mode {
                MatchMode::All => results.iter().all(|&r| r),
                MatchMode::Any => results.iter().any(|&r| r),
            }
        };

        // 组合结果
        match group.match_mode {
            MatchMode::All => conditions_result && groups_result,
            MatchMode::Any => {
                if group.conditions.is_empty() && group.groups.is_empty() {
                    true
                } else {
                    conditions_result || groups_result
                }
            }
        }
    }

    /// 处理命令
    async fn handle_command(&mut self, cmd: RuleCommand) -> bool {
        match cmd {
            RuleCommand::Add(rule, reply) => {
                let id = rule.id.clone();
                let is_clock = rule.is_clock_rule();
                let cond_count = Self::count_conditions(&rule);

                // 添加评估状态
                self.eval_states.insert(
                    id.clone(),
                    RuleEvalState {
                        rule_id: id.clone(),
                        condition_start_times: vec![None; cond_count],
                        last_triggered_at: None,
                    },
                );

                // 如果是时钟规则，初始化时钟状态
                if is_clock && rule.enabled {
                    if let Some(cron_expr) = Self::find_cron_expression(&rule) {
                        if let Some(next_ms) = Self::compute_next_cron_match(&cron_expr) {
                            self.clock_states.push(ClockRuleState {
                                rule_id: id.clone(),
                                next_match_at_ms: next_ms,
                            });
                        }
                    }
                }

                self.rules.insert(id, rule);
                let _ = reply.send(Ok(()));
            }
            RuleCommand::Update(id, patch, reply) => {
                if let Some(rule) = self.rules.get_mut(&id) {
                    let was_clock = rule.is_clock_rule();
                    let was_enabled = rule.enabled;

                    rule.apply_patch(patch);

                    let is_clock = rule.is_clock_rule();
                    let is_enabled = rule.enabled;

                    // 更新时钟状态
                    if was_clock && (!is_clock || !is_enabled) {
                        // 移除时钟状态
                        self.clock_states.retain(|s| s.rule_id != id);
                    } else if is_clock && is_enabled && (!was_clock || !was_enabled) {
                        // 添加时钟状态
                        if let Some(cron_expr) = Self::find_cron_expression(rule) {
                            if let Some(next_ms) = Self::compute_next_cron_match(&cron_expr) {
                                self.clock_states.push(ClockRuleState {
                                    rule_id: id.clone(),
                                    next_match_at_ms: next_ms,
                                });
                            }
                        }
                    }

                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(anyhow::anyhow!("Rule not found: {}", id)));
                }
            }
            RuleCommand::Remove(id, reply) => {
                if self.rules.remove(&id).is_some() {
                    self.eval_states.remove(&id);
                    self.clock_states.retain(|s| s.rule_id != id);
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(anyhow::anyhow!("Rule not found: {}", id)));
                }
            }
            RuleCommand::RunNow(id, reply) => {
                // 并发保护：如果规则正在执行，返回错误
                if self.running_rules.contains(&id) {
                    let _ = reply.send(Err(anyhow::anyhow!("Rule {} is already running", id)));
                } else if let Some(rule) = self.rules.get(&id).cloned() {
                    // 标记为正在执行
                    self.running_rules.insert(id.clone());

                    let request = ExecutionRequest {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        rule_description: rule.description.clone(),
                        message: rule.message.clone(),
                        timeout_seconds: rule.timeout_seconds,
                        context: Some("手动触发".to_string()),
                        trigger_source: "manual".to_string(),
                    };

                    let result = self.executor.execute(request).await;

                    // 交付结果
                    self.executor.deliver(
                        &rule.delivery_channel,
                        &rule.delivery_target,
                        &id,
                        &result,
                    );

                    // 移除正在执行标记
                    self.running_rules.remove(&id);

                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err(anyhow::anyhow!("Rule not found: {}", id)));
                }
            }
            RuleCommand::List(reply) => {
                let rules: Vec<Rule> = self.rules.values().cloned().collect();
                let _ = reply.send(rules);
            }
            RuleCommand::ListSources(reply) => {
                let sources: Vec<EventSource> = self.sources.values().cloned().collect();
                let _ = reply.send(sources);
            }
            RuleCommand::Status(reply) => {
                // 收集每个规则的详细状态
                let rules: Vec<RuleStatusInfo> = self.rules.values().map(|rule| {
                    // 获取 plan 状态
                    let plan_status = self.executor.get_plan_status(&rule.id);
                    let plan = if plan_status.exists {
                        Some(PlanCacheStatus {
                            exists: true,
                            path: plan_status.path,
                            created_at_ms: plan_status.created_at_ms,
                            steps_count: plan_status.steps_count,
                        })
                    } else {
                        None
                    };

                    // 获取 last_run 状态
                    let last_run = self.executor.get_last_run(&rule.id).map(|info| LastRunStatus {
                        source: info.source,
                        status: info.status,
                        duration_ms: info.duration_ms,
                        started_at_ms: info.started_at_ms,
                    });

                    RuleStatusInfo {
                        id: rule.id.clone(),
                        name: rule.name.clone(),
                        enabled: rule.enabled,
                        plan,
                        last_run,
                    }
                }).collect();

                let status = EngineStatus {
                    total_rules: self.rules.len(),
                    enabled_rules: self.rules.values().filter(|r| r.enabled).count(),
                    next_clock_wake_ms: self.compute_next_clock_wake(),
                    active_sources: self.sources.len(),
                    rules,
                };
                let _ = reply.send(status);
            }
            RuleCommand::Shutdown => {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_ring_buffer() {
        let mut buffer = RingBuffer::new(5000); // 5 秒窗口

        let now = chrono::Utc::now().timestamp_millis();
        let event1 = EventData {
            source_id: "test".to_string(),
            timestamp_ms: now - 500,
            fields: HashMap::new(),
        };
        let event2 = EventData {
            source_id: "test".to_string(),
            timestamp_ms: now,
            fields: HashMap::new(),
        };

        buffer.push(event1);
        buffer.push(event2);

        let range = buffer.get_range(now - 500, now);
        assert_eq!(range.len(), 2);
    }

    #[test]
    fn test_compute_next_cron_match() {
        // 每分钟触发
        let result = RuleEngine::compute_next_cron_match("* * * * *");
        assert!(result.is_some());

        // 无效表达式
        let result = RuleEngine::compute_next_cron_match("invalid");
        assert!(result.is_none());
    }

    #[test]
    fn test_value_to_f64() {
        let num = serde_json::json!(42.5);
        assert_eq!(RuleEngine::value_to_f64(&num), Some(42.5));

        let str_num = serde_json::json!("123.45");
        assert_eq!(RuleEngine::value_to_f64(&str_num), Some(123.45));

        let invalid = serde_json::json!("not a number");
        assert_eq!(RuleEngine::value_to_f64(&invalid), None);
    }

    #[test]
    fn test_value_to_string() {
        let str_val = serde_json::json!("hello");
        assert_eq!(RuleEngine::value_to_string(&str_val), "hello");

        let num_val = serde_json::json!(42);
        assert_eq!(RuleEngine::value_to_string(&num_val), "42");

        let bool_val = serde_json::json!(true);
        assert_eq!(RuleEngine::value_to_string(&bool_val), "true");
    }
}
