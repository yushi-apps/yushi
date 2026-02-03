mod llm;
mod chat;
mod tool;
mod agent;
mod query;
pub mod messages;
mod task_tool;

mod error;

pub use chat::{chat, chat_stream};
pub use llm::ModelProfile;
pub use tool::{Tool, ToolUseContext, ToolMessage, ToolResult};
pub use error::{JieyushaError, Result};

use std::sync::{OnceLock, Arc, RwLock};
use std::collections::HashMap;
use task_tool::TaskTool;
pub struct Registry {
    system_prompt: RwLock<String>,
    agent_prompt: RwLock<String>,
    models: RwLock<HashMap<String, ModelProfile>>,
    agents: RwLock<HashMap<String, agent::AgentConfig>>,
    tools: RwLock<HashMap<String, Arc<dyn tool::Tool>>>
}

impl Registry {
    pub fn instance() -> &'static Registry {
        static INSTANCE: OnceLock<Arc<Registry>> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let registry = Arc::new(Registry {
                system_prompt: RwLock::new(String::new()),
                agent_prompt: RwLock::new(String::new()),
                models: RwLock::new(HashMap::new()),
                agents: RwLock::new(HashMap::new()),
                tools: RwLock::new(HashMap::new()),
            });

            //registry.register_tool(Arc::new(TaskTool));
            registry
        })
    }

    pub fn register_model(&self, profile: &ModelProfile) {
        let mut models = self.models.write().unwrap();

        // First model is default main model
        if models.is_empty() {
            models.insert("main".to_string(), profile.clone());
        }

        models.insert(profile.model_name.clone(), profile.clone());
    }

    pub fn register_main_model(&self, profile: ModelProfile) {
        self.models.write().unwrap().insert("main".to_string(), profile);
    }

    pub fn get_model_profile(&self, name: &str) -> Option<ModelProfile> {
        self.models.read().unwrap().get(name).cloned()
    }

    pub fn register_tool(&self, tool: Arc<dyn tool::Tool>) {
        self.tools.write().unwrap().insert(tool.name().to_string(), tool);
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn tool::Tool>> {
        self.tools.read().unwrap().get(name).cloned()
    }

    pub fn get_all_tools(&self) -> Vec<Arc<dyn tool::Tool>> {
        self.tools.read().unwrap().values().cloned().collect()
    }

    pub fn register_system_prompt(&self, prompt: String) {
        *self.system_prompt.write().unwrap() = prompt;
    }

    pub fn get_system_prompt(&self) -> String {
        self.system_prompt.read().unwrap().clone()
    }

    pub fn register_agent_prompt(&self, prompt: String) {
        *self.agent_prompt.write().unwrap() = prompt;
    }

    pub fn get_agent_prompt(&self) -> String {
        self.agent_prompt.read().unwrap().clone()
    }

    pub fn load_all_agents(&self, agent_dir: &str) -> error::Result<()>{
        let agents = agent::scan_agent_directory(agent_dir)?;
        for agent in agents {
            self.agents.write().unwrap().insert(agent.agent_type.clone(), agent);
        }
        Ok(())
    }

    pub fn register_agent(&self, agent_config: &str) {
        if let Some(agent) = agent::parse_agent_config(agent_config) {
            self.agents.write().unwrap().insert(agent.agent_type.clone(), agent);
        }
    }

    pub fn get_all_agents(&self) -> Vec<agent::AgentConfig> {
        self.agents.read().unwrap().values().cloned().collect()
    }

    pub fn get_agent(&self, agent_type: &str) -> Option<agent::AgentConfig> {
        self.agents.read().unwrap().get(agent_type).cloned()
    }

    pub fn get_all_agent_types(&self) -> Vec<String> {
        self.agents.read().unwrap().keys().cloned().collect()
    }
}