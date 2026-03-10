# 可逆计算任务系统实现方案

## 一、核心概念

### 1.1 可逆计算原理

- **差量(Delta)**: memory.xdef的XML实例文件，每次LLM交互或工具调用产生一个实例
- **合并(Merge)**: 通过xdsl.xdef定义的规则(merge/append/prepend/replace/delete)将多个XML实例合并
- **一个App = 一个任务**: 每个独立应用对应一个完整的任务生命周期
- **补偿操作**: 撤销不使用逆差量，而是通过补偿动作实现（如发送取消邮件）

### 1.2 memory.xdef结构回顾

```xml
<Memory id="!string" version="string" created-at="!long" status="enum::SUCCESS, PENDING, FAILED">
    <intent/>                   <!-- 用户意图 -->
    <system-prompt/>            <!-- 系统提示 -->
    <workspace>                 <!-- 工作目录相关文件描述(仅提供信息给LLM，不执行IO) -->
        <file name="!string" description="string"/>
    </workspace>
    <tools>                     <!-- 可用工具 -->
        <tool name="!string" description="string" input-schema="json"/>
    </tools>
    <skills>                    <!-- 可用技能 -->
        <skill name="!string" description="string"/>
    </skills>
    <current>                   <!-- 最近10个action(LLM上下文窗口) -->
        <action id="!string" xdef:ref="Action"/>
    </current>
    <history>                   <!-- 所有action记录 -->
        <action id="!string" xdef:ref="Action"/>
    </history>
</Memory>
```

### 1.3 Action结构

```xml
<Action id="!string" name="!string" type="!enum::thought, tool-call, tool-result, system-message">
    <arguments xdef:value="json"/>
    <result is-summary="boolean" status="enum::OK, ERROR" output="string" error="string">
        <raw-content xdef:value="string"/>  <!-- 工具输出完整内容 -->
    </result>
</Action>
```

**说明**: 
- `output`: 摘要信息，供LLM上下文使用
- `raw-content`: 工具输出的完整内容

---

## 二、文件系统目录结构

### 2.1 运行时任务目录结构

运行时目录以 `App::root_path()` 为根目录（`~/.yushi` 或项目根目录）：

```
{root_path}/                        # App::root_path() 返回值
├── history/                        # 差量历史目录(一个App = 一个任务)
│   ├── base.xml                    # 初始Memory实例
│   ├── 001_{action_id}.xml         # 第1步差量(包含工具输出内容)
│   ├── 002_{action_id}.xml         # 第2步差量
│   ├── ...
│   ├── current.xml                 # 当前合并状态(缓存)
│   └── cli.txt                     # CLI历史
└── agents/                         # 子代理定义
    └── delta_agent.md
```

**说明**:
- 运行时目录基于 `App::root_path()`，与现有App结构保持一致
- 差量文件直接存放在history目录，无需额外app_id层级
- 工具输出内容直接合并到差量XML的result节点中
- CLI历史文件cli.txt存放在history目录下

---

## 三、数据流设计

### 3.1 工具结果文件注入流程

```
用户输入 → LLM规划 → 工具调用 → 工具执行 → 获取结果内容
                                        ↓
                              Delta Agent生成摘要
                                        ↓
                              创建差量XML(工具输出内容嵌入result节点)
                                        ↓
                              返回摘要给LLM
```

### 3.2 上下文稳定性

```
LLM接收的上下文 = workspace状态描述 + 最近10步action历史
                        ↓
                 长度几乎固定(可预测)
```

### 3.3 差量合并流程

```
base.xml + 001.xml + 002.xml + ... → current.xml
    ↓           ↓          ↓
 初始状态   使用xdsl merge规则合并   当前完整状态
```

**差量存储位置**: `.yushi/history/`

---

## 四、模块实现方案

### 4.1 新增模块

#### delta_agent.rs

**职责**:
- 调用LLM生成工具结果摘要
- 管理差量文件的创建
- 提供稳定的LLM上下文

**核心接口**:
```rust
pub struct DeltaAgent { ... }

impl DeltaAgent {
    /// 处理工具执行结果
    async fn process_tool_result(&mut self, action: &Action, content: &str) -> Result<DeltaResult>;
    
    /// 生成摘要(调用LLM)
    async fn generate_summary(&self, tool_name: &str, content: &str) -> Result<String>;
    
    /// 获取LLM上下文(workspace + 最近10步)
    fn get_llm_context(&self) -> LlmContext;
    
    /// 合并所有差量
    fn merge_all(&self) -> Result<Memory>;
}
```

#### summarizer.rs

**职责**:
- 封装摘要生成逻辑
- 针对不同工具类型使用不同摘要策略

**核心接口**:
```rust
pub struct Summarizer { ... }

impl Summarizer {
    /// 生成工具结果摘要
    async fn summarize(&self, tool_name: &str, content: &str, task_context: &str) -> Result<String>;
    
    /// 生成文件描述
    async fn describe_file(&self, filename: &str, preview: &str) -> Result<String>;
}
```

### 4.2 修改模块

#### tool.rs

- 工具结果直接返回内容，由DeltaAgent负责嵌入差量
- 摘要生成移交给DeltaAgent处理

#### query.rs

- `execute_tool_with_file_injection()`集成DeltaAgent
- 工具执行后通过DeltaAgent保存结果和生成摘要

#### context/manager.rs

- 支持从差量链加载（使用tuo/xdsl模块）
- 实现补偿操作（替代逆差量的撤销方式）
- 删除对result_writer的依赖

#### context/context.rs

- 增加`merge()`方法支持与另一个Memory实例合并
- 增加`load_with_deltas()`从基础+差量加载

---

## 五、Delta Agent Prompt设计

### 5.1 系统Prompt

```
你是Delta Agent，基于可逆计算原理的上下文管理专家。

## 核心职责
1. 为工具执行结果生成简洁摘要
2. 确保LLM接收的上下文长度稳定

## 摘要生成原则
- 摘要必须包含：操作类型、主要结果
- 长度控制在200-500字
- 保留关键数据：数值、名称、错误信息
- 标注信息来源和关联步骤

## 输出格式

工具结果摘要:
<summary>
  <action>{操作描述}</action>
  <result>{主要结果}</result>
</summary>
```

### 5.2 场景化Prompt模板

#### 网页内容摘要
```
总结网页内容，提取：
- 页面标题
- 关键信息(3-5条)
- 与当前任务相关的数据
```

#### 代码执行摘要
```
总结代码执行结果：
- 退出状态
- 关键输出(省略冗长日志)
- 错误信息(如有，完整保留)
```

#### 搜索结果摘要
```
总结搜索结果：
- 结果数量
- 前3条关键结果
- 与任务相关的发现
```

---

## 六、测试场景实现

### 6.1 场景：房屋搜索自动化

**用户需求**: "每周五下午追踪Zillow公寓，满足条件时自动发邮件预约看房"

#### 步骤1: 初始上下文 (base.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" id="app-zillow-001" version="1" 
        created-at="1704110400000" status="PENDING">
    <intent>每周五下午追踪Zillow收藏区域公寓，满足条件(租金&lt;3000美元、2居室以上、有阳台)时自动发邮件联系房东预约周末看房，并创建Google日历事件</intent>
    <system-prompt>你是一个自动化房屋搜索助手，帮助用户追踪房源并安排看房</system-prompt>
    <workspace/>
    <tools>
        <tool name="WebSearch" description="搜索网页"/>
        <tool name="WebFetch" description="获取网页内容"/>
        <tool name="SendEmail" description="发送邮件"/>
        <tool name="Calendar" description="管理日历事件"/>
        <tool name="HumanApproval" description="请求用户确认"/>
    </tools>
    <skills/>
    <current/>
    <history/>
</Memory>
```

#### 步骤2: LLM规划 (001_thought.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="../base.xml">
    <history x:override="append">
        <action id="thought-001" name="thought" type="thought">
            <result status="OK" output="分析任务：1)搜索Zillow符合条件房源 2)筛选满足条件的 3)生成邮件草案 4)请求用户确认 5)发送邮件 6)创建日历事件"/>
        </action>
    </history>
    <current x:override="append">
        <action id="thought-001" name="thought" type="thought">
            <result status="OK" output="分析任务：1)搜索Zillow符合条件房源 2)筛选满足条件的 3)生成邮件草案 4)请求用户确认 5)发送邮件 6)创建日历事件"/>
        </action>
    </current>
</Memory>
```

#### 步骤3: 工具调用 (002_tool_call.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="001_thought.xml">
    <history x:override="append">
        <action id="call-002" name="WebSearch" type="tool-call">
            <arguments>{"query": "Zillow apartments San Francisco under $3000 2 bedroom balcony"}</arguments>
        </action>
    </history>
</Memory>
```

#### 步骤4: 工具结果 (003_tool_result.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="002_tool_call.xml">
    <history x:override="append">
        <action id="result-003" name="WebSearch" type="tool-result">
            <result is-summary="true" status="OK" output="找到3个符合条件的公寓：A123($2800,2BR,阳台)、B456($2650,2BR,阳台)、C789($2900,3BR,阳台)">
                <raw-content>
搜索结果完整内容...
1. A123公寓 - $2800/月, 2卧室, 带阳台, 地址: xxx
2. B456公寓 - $2650/月, 2卧室, 带阳台, 地址: xxx
3. C789公寓 - $2900/月, 3卧室, 带阳台, 地址: xxx
                </raw-content>
            </result>
        </action>
    </history>
</Memory>
```

#### 步骤5: 生成邮件草案 (004_thought.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="003_tool_result.xml">
    <history x:override="append">
        <action id="thought-004" name="thought" type="thought">
            <result status="OK" output="已生成3封邮件草案，等待用户确认后发送">
                <raw-content>
邮件草案1: 致A123房东...
邮件草案2: 致B456房东...
邮件草案3: 致C789房东...
                </raw-content>
            </result>
        </action>
    </history>
</Memory>
```

#### 步骤6: 请求用户确认 (005_tool_call.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="004_thought.xml">
    <history x:override="append">
        <action id="call-005" name="HumanApproval" type="tool-call">
            <arguments>{"message": "是否确认发送3封看房预约邮件？", "options": ["全部发送", "选择发送", "取消"]}</arguments>
        </action>
    </history>
</Memory>
```

#### 步骤7: 用户确认 (006_tool_result.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="005_tool_call.xml">
    <history x:override="append">
        <action id="result-006" name="HumanApproval" type="tool-result">
            <result status="OK" output="用户选择：全部发送"/>
        </action>
    </history>
</Memory>
```

#### 步骤8: 发送邮件 (007_tool_result.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="006_tool_result.xml">
    <history x:override="append">
        <action id="call-007" name="SendEmail" type="tool-call">
            <arguments>{"to": ["a123@landlord.com","b456@landlord.com","c789@landlord.com"], "subject": "看房预约请求"}</arguments>
        </action>
        <action id="result-007" name="SendEmail" type="tool-result">
            <result is-summary="true" status="OK" output="成功发送3封邮件">
                <raw-content>
发送记录:
- a123@landlord.com: 发送成功
- b456@landlord.com: 发送成功
- c789@landlord.com: 发送成功
                </raw-content>
            </result>
        </action>
    </history>
</Memory>
```

#### 步骤9: 任务暂停等待回复 (008_system.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="007_tool_result.xml" status="PENDING">
    <history x:override="append">
        <action id="system-008" name="system" type="system-message">
            <result status="OK" output="任务暂停，等待房东回复邮件"/>
        </action>
    </history>
</Memory>
```

#### 步骤10: 收到房东回复 (009_system.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="008_system.xml">
    <history x:override="append">
        <action id="system-009" name="system" type="system-message">
            <result status="OK" output="收到房东A123回复：同意周六下午2点看房">
                <raw-content>
From: a123@landlord.com
Subject: Re: 看房预约请求
Content: 您好，我同意您的看房请求，时间定在本周六下午2点...
                </raw-content>
            </result>
        </action>
    </history>
</Memory>
```

#### 步骤11: 创建日历事件 (010_tool_result.xml)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="009_system.xml">
    <history x:override="append">
        <action id="call-010" name="Calendar" type="tool-call">
            <arguments>{"event": "看房-A123公寓", "time": "2024-01-06T14:00:00", "location": "A123地址"}</arguments>
        </action>
        <action id="result-010" name="Calendar" type="tool-result">
            <result status="OK" output="已创建日历事件：周六14:00看房A123"/>
        </action>
    </history>
</Memory>
```

#### 步骤12: 撤销操作(补偿方式)

假设用户后悔发送给B456的邮件，系统使用补偿操作（非逆差量）：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="010_tool_result.xml">
    <history x:override="append">
        <action id="thought-011" name="thought" type="thought">
            <result status="OK" output="用户请求撤销发送给B456的邮件，由于邮件已发送无法物理撤回，执行补偿操作：发送取消邮件"/>
        </action>
        <action id="call-011" name="SendEmail" type="tool-call">
            <arguments>{"to": "b456@landlord.com", "subject": "取消看房预约"}</arguments>
        </action>
        <action id="result-011" name="SendEmail" type="tool-result">
            <result status="OK" output="已发送取消邮件给B456">
                <raw-content>
To: b456@landlord.com
Subject: 取消看房预约
Content: 抱歉，我需要取消之前的看房预约请求...
                </raw-content>
            </result>
        </action>
    </history>
</Memory>
```

---

## 七、实现步骤

### Phase 1: 基础架构

1. **创建delta_agent.rs**
   - 实现DeltaAgent结构体
   - 实现差量创建逻辑
   - 集成tuo/xdsl模块进行合并

2. **创建summarizer.rs**
   - 实现Summarizer结构体
   - 实现LLM摘要调用

3. **配置Delta Agent**
   - 创建{root_path}/agents/delta_agent.md
   - 设计系统Prompt

### Phase 2: 模块集成

4. **修改tool.rs**
   - 工具结果直接返回内容
   - 由DeltaAgent负责嵌入差量和生成摘要

5. **修改query.rs**
   - 集成DeltaAgent
   - 工具执行后调用DeltaAgent处理结果

6. **修改context/manager.rs**
   - 支持差量链加载和合并
   - 实现补偿操作接口

### Phase 3: 测试验证

7. **编写单元测试**
   - 差量合并测试
   - 摘要生成测试

8. **场景测试**
   - 执行房屋搜索场景的12个步骤
   - 验证补偿操作

---

## 八、周期性摘要生成

### 8.1 设计原则

当 `history` 中的 action 数量超过阈值（默认10条）时，Delta Agent 自动触发摘要生成：

- **history**: 保留完整记录（原始 action + 摘要 action），**永不删除**
- **current**: 优化后的 LLM 上下文（摘要 action + 最近 N 个 action）
- **差量文件链**: 保持完整，不压缩

### 8.2 触发条件

```rust
fn should_generate_summary(&self) -> bool {
    self.context.history.len() > 10
}
```

### 8.3 生成流程

```
history: [a1, a2, a3, ..., a10, a11]
                    ↓ 触发摘要
history: [a1, a2, a3, ..., a10, a11, summary-action]  // 原始不删除，摘要追加
current: [summary-action, a7, a8, a9, a10, a11]       // 摘要+最近5个
```

### 8.4 摘要 Action 结构

```xml
<action id="action-summary-xxx" name="_historical_summary" type="historical-summary">
    <arguments>
        {"summarized_count": 6, "summary_method": "llm_summarization"}
    </arguments>
    <result is-summary="true" status="OK">
        [历史摘要内容：总结了a1-a6的主要工作、关键结果、重要发现]
    </result>
</action>
```

### 8.5 current 计算规则

`current` 字段始终通过以下规则计算：

```rust
fn update_current(&mut self) {
    self.current.clear();
    
    // 找到最近的 historical-summary action
    let summary_idx = self.history.iter().rposition(|a| a.action_type == HistoricalSummary);
    
    // current = 摘要 + 摘要之后的 action（最多保留10个）
    if let Some(idx) = summary_idx {
        self.current.push(self.history[idx].clone());
        for action in self.history.iter().skip(idx + 1).take(9) {
            self.current.push(action.clone());
        }
    } else {
        // 无摘要，取最近10个
        let start = self.history.len().saturating_sub(10);
        for action in self.history.iter().skip(start) {
            self.current.push(action.clone());
        }
    }
}
```

### 8.6 LLM 摘要 Prompt

```
你是 Delta Agent 的历史摘要生成模块。你的任务是将多个历史动作压缩为简洁的摘要。

## 摘要生成原则
- 摘要必须包含：完成的主要工作、关键的输出和文件、重要的发现或结果
- 长度控制在200-500字
- 保留关键数据：数值、名称、错误信息
- 标注信息来源和关联步骤
- 按时间顺序组织内容

## 输出格式
直接输出摘要文本，不需要任何标记或格式化。
```

### 8.7 存储示例

假设执行了12步操作后触发摘要：

**history 状态**:
```
[a1-thought, a2-tool-call, a3-tool-result, ..., a11-thought, a12-summary]
```

**current 状态**:
```
[a12-summary, a8-tool-result, a9-thought, a10-tool-call, a11-thought]
```

**差量文件链**:
```
001_thought.xml → 002_tool_call.xml → ... → 012_summary.xml
```

所有差量文件保持完整，可通过 `load_with_deltas()` 完整恢复历史。

---

## 九、注意事项

1. **禁止逆差量**: 撤销操作必须通过补偿动作实现，不使用逆差量

2. **Workspace仅作信息提供**: workspace用于描述工作目录相关文件，仅提供信息给LLM，不执行实际读写，不记录差量xml文件

3. **上下文长度稳定**: LLM接收的上下文 = workspace描述 + 最近10步history

4. **差量即Memory实例**: 不额外定义差量格式，直接使用memory.xdef的XML实例

5. **工具输出内容嵌入差量**: 工具执行结果直接存储在Action的result/raw-content节点中

6. **差量文件统一存储**: 所有差量文件存放在.yushi/history/目录下，CLI历史cli.txt也在该目录
