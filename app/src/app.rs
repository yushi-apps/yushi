use std::sync::Arc;
use std::fs::File;
use actix_web::{HttpServer, App as ActixApp};
use uuid::Uuid;
use jieyusha::{Tool, Registry, ModelProfile};
use crate::services;

pub struct App {
    name: String
}

impl Default for App {
    fn default() -> Self {
        App {
            name: "DaYuHai".to_string()
        }
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
        Registry::instance().register_agent(agent_config);
        self
    }
}

