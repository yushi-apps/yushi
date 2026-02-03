use std::env;
use std::sync::Arc;
use std::fs::File;
use std::path::PathBuf;
use actix_web::{HttpServer, App as ActixApp};
use uuid::Uuid;
use jieyusha::{Tool, Registry, ModelProfile};
use crate::services;

pub struct App {
    name: String
}

impl Default for App {
    fn default() -> Self {
        let mut app = App {
            name: "DaYuHai".to_string()
        };

        if let Ok(home) = env::var("HOME") {
            let root = PathBuf::from(home).join(".yushi");

            let app_prompt_path = root.join("app_prompt.md");
            if let Ok(prompt) = std::fs::read_to_string(app_prompt_path) {
                app.add_prompt(&prompt);
            };

            let agent_prompt_path = root.join("agent_prompt.md");
            if let Ok(prompt) = std::fs::read_to_string(agent_prompt_path) {
                app.add_agent_prompt(&prompt);
            };

            let model_path = root.join("config/model.toml");
            if let Ok(profile) = std::fs::read_to_string(model_path) {
                app.add_model(&profile);
            } 
            //if let Some(profile) = app.load_model_profile(&model_path) {
            //    Registry::instance().register_model(&profile);
            //};
            if let Some(agents_dir) = root.join("agents").to_str() {
                Registry::instance().load_all_agents(&agents_dir).expect("Failed to load agents");
            }
        }
        app
    }
}


impl App {
    pub fn new() -> App {
        App::default()
    }

    pub fn add_tools(&mut self, tool: impl Tool + 'static) -> &mut Self{
        Registry::instance().register_tool(Arc::new(tool));
        self
    }

    #[actix_web::main] 
    pub async fn run(&self) -> std::io::Result<()> {
        #[cfg(feature = "model-smallthinker")]
        {
            std::process::Command::new("smallthinker")
                .args([
                    "-m", "/usr/share/yushi/model.gguf",
                    "-c", "2048",
                    "--host", "127.0.0.1",
                    "--port", "22789",
                    "-t", "4",
                    "--jinja",
                    "--chat-template", "chatml",
                    "--repeat-penalty", "1.1",
                    "--offline",
                ])
                .spawn()
                .expect("Failed to start smallthinker server.");
        }

        log::info!("Model Configuration: {:?}", Registry::instance().get_model_profile("main"));

        HttpServer::new(|| {
            ActixApp::new()
                .service(services::chat)
        })
        .bind("0.0.0.0:22786")?
        .run()
        .await
    }

    pub async fn chat(&self, input: &str) -> String {
        let id = Uuid::new_v4().to_string();
        match jieyusha::chat(input, &id).await {
            Ok(output) => output,
            Err(err) => format!("Error: {}", err),
        }
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
        
        let file = File::create(self.name.clone() + ".log").expect("Failed to create log file");
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

