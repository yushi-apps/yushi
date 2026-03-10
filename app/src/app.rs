use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use jieyusha::{Tool, Registry, ModelProfile, SkillTool};
use jieyusha::messages::Message;
use jieyusha::memory::Memory;

pub struct App {
    name: String,
}

impl Default for App {
    fn default() -> Self {
        let mut app = App {
            name: "DaYuHai".to_string(),
        };

        let root = Self::root_path();

        let skills_dir = root.join("skills");
        let skills = SkillTool::load_skills(&skills_dir);

        let app_prompt_path = root.join("YUSHI.md");
        if let Ok(prompt) = std::fs::read_to_string(app_prompt_path) {
            let prompt = format!("{}\n{}", prompt, skills);
            app.add_prompt(&prompt);
        } else {
            app.add_prompt(&skills);
        };

        let agent_prompt_path = root.join("AGENTS.md");
        if let Ok(prompt) = std::fs::read_to_string(agent_prompt_path) {
            app.add_agent_prompt(&prompt);
        };

        let model_path = root.join("config/model.toml");
        if let Ok(profile) = std::fs::read_to_string(model_path) {
            app.add_model(&profile);
        }

        if let Some(agents_dir) = root.join("agents").to_str() {
            let path = PathBuf::from(agents_dir);
            if path.exists() && path.is_dir() {
                Registry::instance().load_all_agents(agents_dir).expect("Failed to load agents");
            }
        }

        Memory::init_base().unwrap();

        app
    }
}


impl App {
    pub fn new() -> App {
        App::default()
    }

    pub fn root_path() -> PathBuf {
        if let Ok(home) = env::var("HOME") {
            let yushi_path = PathBuf::from(home).join(".yushi");
            if yushi_path.exists() {
                yushi_path
            } else {
                let manifest_dir = env!("CARGO_MANIFEST_DIR");
                PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
            }
        } else {
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
        }
    }
    
    /// 获取 AGENTS.md 文件路径
    pub fn agents_md() -> PathBuf {
        Self::root_path().join("AGENTS.md")
    }
    
    /// 获取 agents 目录路径
    pub fn agents_dir() -> PathBuf {
        Self::root_path().join("agents")
    }
    
    /// 获取 History 目录路径
    pub fn history_dir() -> PathBuf {
        Self::root_path().join("history")
    }
    
    pub fn add_tools(&mut self, tool: impl Tool + 'static) -> &mut Self{
        Registry::instance().register_tool(Arc::new(tool));
        self
    }
    
    pub fn trace(&mut self, level: &str) -> &mut Self {
        let tracing_level = match level {
            "ERROR" => tracing::Level::ERROR,
            "WARN" => tracing::Level::WARN,
            "INFO" => tracing::Level::INFO,
            "DEBUG" => tracing::Level::DEBUG,
            "TRACE" => tracing::Level::TRACE,
            _ => tracing::Level::ERROR,
        };
        
        let file = fs::File::create(self.name.clone() + ".log").expect("Failed to create log file");
        tracing_subscriber::fmt()
            .with_max_level(tracing_level) 
            .with_target(false)      
            .with_thread_ids(false)  
            .with_thread_names(false) 
            .with_file(false)        
            .with_line_number(false) 
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE)
            .with_ansi(false) 
            .with_writer(file)
            .init();

        log::debug!("Default Registry: {}", Registry::instance());
        self
    }

    pub fn add_prompt(&mut self, prompt: impl Into<String>) -> &mut Self {
        Registry::instance().register_system_prompt(prompt.into());
        self 
    }

    pub fn add_agent_prompt(&mut self, prompt: impl Into<String>) -> &mut Self {
        Registry::instance().register_agent_prompt(prompt.into());
        self 
    }

    pub fn add_model(&mut self, model_profile: &str) -> &mut Self {
        if let Ok(value) = model_profile.parse::<toml::Table>() {
            let model = &value["model"];
            let model_api = &value["model"]["api"];
            let model_parameters = &value["model"]["parameters"];

            if let (Some(name), Some(base_url), Some(api_key), Some(max_tokens), Some(temperature)) = (
                model["name"].as_str(),
                model_api["base_url"].as_str(),
                model_api["api_key"].as_str(),
                model_parameters["max_tokens"].as_integer(),
                model_parameters["temperature"].as_float(),
            ) {
                log::debug!("Temperature: {}", temperature);
                let profile = ModelProfile::profile()
                    .model_name(name)
                    .base_url(base_url)
                    .api_key(api_key)
                    .max_tokens(max_tokens as u32)
                    .temperature(temperature as f32)
                    .build();

                Registry::instance().register_model(&profile);
            }
        }
        self 
    }

    pub fn add_agent(&mut self, agent_config: &str) -> &mut Self {
        log::debug!("Register agent: {}", agent_config);
        Registry::instance().register_agent(agent_config);
        self
    }
}
