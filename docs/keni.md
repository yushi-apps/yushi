# 可逆计算差量文件方案

## 1. 核心原则

**一切皆差量，永不覆盖**

- base 变化 → 生成 base 差量
- intent 输入 → 生成 intent 差量
- tool 调用 → 生成 tool 差量
- 周期性摘要 → 生成 summary 差量

## 2. 文件命名规范

**序号格式**：10进制数字，省略前导零

| 序号 | 文件名示例 |
|------|-----------|
| 0 | `0_base.xml` |
| 1 | `1_base.xml` |
| 2 | `2_intent.xml` |
| 10 | `10_tool.xml` |
| 100 | `100_summary.xml` |

**递增规则**：严格 +1 递增

## 3. 文件类型

| 类型 | 文件名模式 | 内容 | 继承 |
|------|-----------|------|------|
| base | `N_base.xml` | system_prompt, tools, skills | 前一个文件 |
| intent | `N_intent.xml` | 用户意图 + thought action | 前一个文件 |
| tool | `N_tool.xml` | tool-call + tool-result | 前一个文件 |
| summary | `N_summary.xml` | 周期性历史摘要 | 前一个文件 |

## 4. 文件结构

```
history/
├── 0_base.xml           # 初始配置
├── 1_base.xml           # 配置更新（自动检测 skills/YUSHI.md 变化）
├── 2_intent.xml         # 第一次用户输入
├── 3_tool.xml           # 工具调用（call + result）
├── 4_tool.xml           # 工具调用
├── 5_intent.xml         # 用户追问
├── 6_tool.xml           # 工具调用
├── 7_base.xml           # 配置更新
├── 8_tool.xml           # 工具调用
├── 9_summary.xml        # 周期性摘要
└── current.xml          # 合并结果（按需生成）
```

## 5. 继承链

```
0_base → 1_base → 2_intent → 3_tool → 4_tool → 5_intent → 6_tool → 7_base → 8_tool → 9_summary → ...
```

每个文件继承前一个文件（N-1），形成严格的链式继承。

## 6. 文件格式

### 6.1 base 文件

**0_base.xml**（初始配置）：
```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory id="uuid" version="1" created-at="timestamp" status="PENDING">
    <system-prompt>初始系统提示</system-prompt>
    <workspace />
    <tools>
        <tool name="Bash" description="Execute shell commands" />
        <tool name="Task" description="Launch a new task" />
    </tools>
    <skills>
        <skill name="weather" description="Get weather info" />
    </skills>
    <current />
    <history />
</Memory>
```

**N_base.xml**（配置更新）：
```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="N-1_xxx.xml">
    <system-prompt>更新后的系统提示</system-prompt>
    <skills x:override="append">
        <skill name="new-skill" description="新增技能" />
    </skills>
</Memory>
```

### 6.2 intent 文件

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="N-1_xxx.xml">
    <intent>用户输入的意图</intent>
    <history x:override="append">
        <action id="action-xxx" name="thought" type="thought">
            <result status="OK" output="用户意图内容" />
        </action>
    </history>
</Memory>
```

### 6.3 tool 文件

包含完整的工具调用和结果：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="N-1_xxx.xml">
    <history x:override="append">
        <action id="call_xxx" name="Bash" type="tool-call">
            <arguments>{"cmd": "cat file.txt"}</arguments>
            <result is-summary="true" status="OK" output="摘要结果">
                <raw-content>完整的工具输出结果</raw-content>
            </result>
        </action>
    </history>
</Memory>
```

### 6.4 summary 文件

周期性摘要，当 history 超过 10 条 action 时生成：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Memory xmlns:x="xdsl.xdef" x:extends="N-1_xxx.xml">
    <history x:override="append">
        <action id="summary-xxx" name="_historical_summary" type="historical-summary">
            <arguments>{"summarized_count": 5}</arguments>
            <result is-summary="true" status="OK" output="历史摘要内容" />
        </action>
    </history>
</Memory>
```

## 7. current.xml 生成

**更新时机**：按需生成（LLM 请求上下文时）

**生成方式**：
1. 扫描 history 目录，按数字大小排序所有文件
2. 从 0_base.xml 开始
3. 依次读取并合并每个差量文件
4. 得到完整 Memory
5. 保存为 current.xml

## 8. base 自动更新

**触发条件**：
- skills 目录内容变化（新增/删除/修改 SKILL.md）
- YUSHI.md 文件修改

**检测方式**：
- 对比当前配置与合并后的配置
- 如有差异，生成新的 N_base.xml

**实现建议**：
- 记录 skills 目录的文件列表和修改时间的 hash
- 记录 YUSHI.md 的内容 hash
- 每次操作前对比，如有变化则生成 base 差量

## 9. 接口设计

```rust
// 初始化
Memory::init_base()                    // 生成 0_base.xml

// 配置更新（自动检测调用）
Memory::check_and_update_base()        // 检测变化，生成 N_base.xml

// 用户交互
Memory::intent(input: &str)            // 生成 N_intent.xml
Memory::tool_action(call, result)      // 生成 N_tool.xml

// 周期性摘要
Memory::summary(actions, content)      // 生成 N_summary.xml

// 工具方法
Memory::get_next_number()              // 获取下一个序号
Memory::get_latest_file()              // 获取最新文件
Memory::load()                         // 合并所有差量，生成 current.xml
Memory::get_llm_context()              // 获取 LLM 上下文
```

## 10. 合并流程

```
读取最新文件（如 9_summary.xml）
    ↓
解析 x:extends，递归加载父文件
    ↓
从 0_base.xml 开始，依次合并
    ↓
应用各文件的 x:override 规则
    ↓
得到完整 Memory
    ↓
保存为 current.xml
```

## 11. x:override 规则

| 规则 | 说明 |
|------|------|
| merge | 合并属性和子节点（默认） |
| append | 追加子节点到末尾 |
| prepend | 前置子节点到开头 |
| replace | 替换整个节点 |
| delete | 删除节点 |

**本方案使用**：
- `<history x:override="append">` - 追加 action
- `<skills x:override="append">` - 追加 skill
- 其他节点默认 merge

## 12. 完整示例

```
任务开始：
  0_base.xml           # 初始配置（tools, skills, system_prompt）

检测到 skills 变化：
  1_base.xml           # x:extends="0_base.xml"，追加新 skill

用户输入："分析这个文件"：
  2_intent.xml         # x:extends="1_base.xml"，添加 intent + thought

工具调用（cat file.txt）：
  3_tool.xml           # x:extends="2_intent.xml"，tool-call + tool-result

工具调用（grep pattern）：
  4_tool.xml           # x:extends="3_tool.xml"

用户追问："再分析另一个"：
  5_intent.xml         # x:extends="4_tool.xml"

工具调用：
  6_tool.xml           # x:extends="5_intent.xml"
  7_tool.xml           # x:extends="6_tool.xml"
  8_tool.xml           # x:extends="7_tool.xml"
  9_tool.xml           # x:extends="8_tool.xml"
  10_tool.xml          # x:extends="9_tool.xml"

检测到 YUSHI.md 修改：
  11_base.xml          # x:extends="10_tool.xml"，更新 system_prompt

工具调用：
  12_tool.xml          # x:extends="11_base.xml"

history 超过 10 条，生成摘要：
  13_summary.xml       # x:extends="12_tool.xml"，historical-summary action

继续...
```

## 13. 优势

| 特性 | 说明 |
|------|------|
| 可追溯 | 每个变化都有记录，可查看何时发生 |
| 可回滚 | 删除某个差量即可回滚到之前状态 |
| 可审计 | 完整记录任务执行过程中的所有变化 |
| 可逆 | 符合可逆计算原理，一切皆差量 |
| 简洁 | 序号连续，命名简单 |
| 自动 | base 更新自动检测，无需手动触发 |
