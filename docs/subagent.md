# Sub-agent

In Yushi, custom sub-agents are specialized AI assistants that can be called upon to handle specific types of tasks. They solve problems more efficiently by providing task-specific customizations such as tailored system prompts, tools, and independent context windows.

## What is a sub-agent

Sub-agents are pre-configured AI assistants to which an agent delegates tasks. Each sub-agent:

- Is designed for a specific purpose and domain of expertise.
- Operates with a context window separate from the agent's main conversation.
- Can be configured to use only a specific set of tools.
- Includes a defined system prompt that guides its behavior.

When an agent encounters a task that matches a sub-agent's expertise, it delegates the task to that sub-agent, which then operates independently and returns the result.

## Quick Start
```sh
cd my_agent
yushi agent new <agent_name>
```

A sub-agent definition file will be created in the `my_agent/.yushi/agents` directory.

### File structure

A sub-agent definition file is a Markdown document with YAML front matter, structured as follows:
``` yaml
---
name: Sub-agent Name
description: Description of when to invoke this sub-agent
tools: tool1, tool2, tool3
---
Write your sub-agent's system prompt here, which may consist of multiple paragraphs.
It should clearly define the sub-agent's role, capabilities, and problem-solving approach.
Include specific instructions, best practices, and any constraints the sub-agent must follow.
```

#### Configuration fields

| Field | Required | Description |
|----------|----------|----------|
|name|Yes|A unique identifier using lowercase letters and underscores|
|description|Yes|A natural-language description of the sub-agent’s capabilities|
|tools|No|Comma-separated list of tools available to the sub-agent|

#### Avaliable tools

A sub-agent can be granted access to any built-in Yushi tool.  
For the full list of available tools, see the [**Tools Documentation**](./tool.md).

## Examples

The [LEGO EV3 Agent](./zh-CN/ev3.md) demo shows how to build a smart toy agent on top of LEGO EV3 using `Yushi`.