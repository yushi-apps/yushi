use std::path::PathBuf;
use std::process::Command;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cargo-yushi",
    bin_name = "cargo yushi",
    about = "No-Code On-Device Agent Development Platform",
    version,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "new", about = "Create a new agent")]
    New {
        name: String,
    },
    #[command(name = "build", about = "Build the agent")]
    Build {
        /// Pass additional arguments to cargo build
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(name = "tool", about = "Manage tools in the agent", subcommand)]
    Tool(ToolCommands),
    #[command(name = "del", about = "Delete an agent")]
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum ToolCommands {
    #[command(name = "add", about = "Add an existing tool to the agent")]
    Add {
        name: String,
    },
    #[command(name = "remove", about = "Remove a tool from the agent")]
    Remove {
        name: String,
    },
    #[command(name = "new", about = "Create a new tool")]
    New {
        name: String,
    },
    #[command(name = "del", about = "Delete a tool")]
    Delete {
        name: String,
    },
}

fn main() {
    // When called via `cargo yushi`, the second argument is "yushi"
    // We need to adjust the arguments to properly parse the subcommands
    let args: Vec<String> = std::env::args().collect();
    
    // If the second argument is "yushi", we skip it for proper parsing
    let adjusted_args = if args.len() > 1 && args[1] == "yushi" {
        let mut new_args = vec![args[0].clone()];
        new_args.extend_from_slice(&args[2..]);
        new_args
    } else {
        args.clone()
    };

    // Parse using the adjusted arguments
    let cli = Cli::parse_from(adjusted_args);

    match &cli.cmd {
        Some(Commands::New { name }) => {
            let output = Command::new("cargo")
                .args(["new", name])
                .output()
                .expect("Failed to execute cargo new command");
            
            if !output.status.success() {
                eprintln!("Failed to create project: {}", String::from_utf8_lossy(&output.stderr));
                std::process::exit(1);
            }
            
            let mut project_path = PathBuf::from(".");
            project_path.push(name);
            
            let cargo_toml_path = project_path.join("Cargo.toml");
            let mut cargo_toml = std::fs::read_to_string(&cargo_toml_path)
                .expect("Failed to read Cargo.toml");
            
            let deps_pos = cargo_toml.find("[dependencies]").unwrap_or(cargo_toml.len());
            let metadata = "description = \"Yushi AI Agent\"\nlicense = \"AGPL-3.0\"\nauthors = [\"Yushi yushi_app@163.com\"]\n";
            if deps_pos < cargo_toml.len() {
                cargo_toml.insert_str(deps_pos - 1, metadata);
            } else {
                cargo_toml.push_str(metadata);
            }
            
            let deps_section = "\n[dependencies]\n";
            if !cargo_toml.contains("[dependencies]") {
                cargo_toml.push_str(deps_section);
            }
            
            cargo_toml.push_str("rust-embed = \"8.9.0\"\n");
            cargo_toml.push_str("app.workspace = true\n");
            cargo_toml.push_str("jieyusha.workspace = true\n");
            cargo_toml.push_str("\n[package.metadata.deb]\n");
            cargo_toml.push_str("assets = [\n");
            cargo_toml.push_str("    [\"target/release/rk3588\", \"usr/bin/\", \"755\"],\n");
            cargo_toml.push_str("]\n");

            std::fs::write(&cargo_toml_path, cargo_toml)
                .expect("Failed to write to Cargo.toml");
            
            let main_rs_path = project_path.join("src").join("main.rs");
            std::fs::write(&main_rs_path, r#"
use rust_embed::RustEmbed;
use app::App;

#[derive(RustEmbed)]
#[folder = "./.yushi/"]
struct YushiAssets;

fn main() {
    let mut app = App::new();
    if let Some(app_prompt) = YushiAssets::get("main_prompt.md") {
        let prompt = std::str::from_utf8(&app_prompt.data).unwrap_or("");
        app.add_prompt(prompt);
    } 

    if let Some(model_file) = YushiAssets::get("model.toml") {
        let profile = std::str::from_utf8(&model_file.data).unwrap_or(""); 
        app.add_model(&profile);
    }

    for relitive_path in YushiAssets::iter() {
        let path = relitive_path.as_ref();
        if path.starts_with("agents/") {
            if let Some(agent_file) = YushiAssets::get(path) {
                let agent_config = std::str::from_utf8(&agent_file.data).unwrap_or("");
                app.add_agent(agent_config);
            }
        }
    }

    app.run().expect("Failed to run application");
}
"#).expect("Failed to write to main.rs");

            let yushi_dir = project_path.join(".yushi");
            std::fs::create_dir_all(&yushi_dir).expect("Failed to create .yushi directory");

            let agents_dir = yushi_dir.join("agents");
            std::fs::create_dir_all(&agents_dir).expect("Failed to create agents directory");

            let app_agent_path = yushi_dir.join("main_prompt.md");
            std::fs::write(&app_agent_path, "").expect("Failed to create .yushi/main_prompt.md");

            let model_path = yushi_dir.join("model.toml");
            std::fs::write(&model_path, r#"
# Model Configuration

[model]
name = "deepseek-chat"
type = "chat"
version = "v3.2"
description = "DeepSeek Chat Model - OpenAI Compatible API"

[model.api]
# Obtain your API key from the official platform of the service provider.
api_key = "your_api_key_here"
# Base URL of the model service.
base_url = "https://api.deepseek.com/chat/completions"

[model.parameters]
max_tokens = 8192 # Maximum tokens for input and output combined.
temperature = 0.2 # Control output randomness: lower values (such as 0.2) are more certain, higher values (such as 0.9) are more creative.
top_p = 0.9       # Nucleus Sampling: Control diversity: 0.5 means only the top 50% of the most probable tokens are considered. 
            "#).expect("Failed to create .yushi/model.toml");

            println!("Successfully created agent at {}", project_path.display());
        }
        
        Some(Commands::Build { args }) => {
            
            let mut cargo_args = vec!["build".to_string()];
            let mut use_cross = false;
            let mut is_release = false;
            
            // Check if --target or --release is in the arguments
            let mut iter = args.iter();
            while let Some(arg) = iter.next() {
                if arg == "--target" {
                    use_cross = true;
                    cargo_args.push(arg.clone());
                    // Also add the target value
                    if let Some(target) = iter.next() {
                        cargo_args.push(target.clone());
                    }
                } else if arg.starts_with("--target=") {
                    use_cross = true;
                    cargo_args.push(arg.clone());
                } else if arg == "--release" {
                    is_release = true;
                    cargo_args.push(arg.clone());
                } else {
                    cargo_args.push(arg.clone());
                }
            }
            
            let mut command = Command::new(if use_cross { "cross" } else { "cargo" });
            for arg in &cargo_args {
                command.arg(arg);
            }

            let status = command
                .status()
                .expect("Failed to execute build command");
                
            if !status.success() {
                eprintln!("Build failed with exit code: {:?}", status.code());
                std::process::exit(status.code().unwrap_or(1));
            }
            
            println!("Successfully built agent");
            
            if is_release {
                println!("Creating Debian package...");
                
                let mut target_arg = None;
                let mut iter = args.iter();
                while let Some(arg) = iter.next() {
                    if arg == "--target" {
                        if let Some(target) = iter.next() {
                            target_arg = Some(target.clone());
                        }
                        break;
                    } else if arg.starts_with("--target=") {
                        target_arg = Some(arg[9..].to_string()); // Skip "--target=" prefix
                        break;
                    }
                }
                
                let mut deb_command = Command::new("cargo");
                deb_command.arg("deb").arg("--no-build");
                
                if let Some(target) = target_arg {
                    deb_command.arg("--target").arg(target);
                }

                let status = deb_command
                    .status()
                    .expect("Failed to execute deb command");
                    
                if !status.success() {
                    eprintln!("Deb packaging failed with exit code: {:?}", status.code());
                    std::process::exit(status.code().unwrap_or(1));
                }
                
                println!("Successfully created Debian package");
            }
        }

        Some(Commands::Tool(tool_cmd)) => match tool_cmd {
            ToolCommands::Add { name } => {
                if !is_upper_camel_case(name) {
                    eprintln!("Error: Tool name must be in UpperCamelCase");
                    std::process::exit(1);
                } 

                let project_root = std::env::current_dir().expect("Failed to get current directory");
                if !project_root.exists() {
                    eprintln!("Error: current directory does not exist");
                    std::process::exit(1);
                }
                
                let cargo_toml_path = project_root.join("Cargo.toml");
                let main_rs_path = project_root.join("src").join("main.rs");
                let yushi_assets_dir = project_root.join(".yushi");
                if !yushi_assets_dir.exists() {
                    eprintln!("Error: This command can only be run from within a agent directory");
                    std::process::exit(1);
                }
                
                let mut cargo_toml = std::fs::read_to_string(&cargo_toml_path)
                    .expect("Failed to read Cargo.toml");
                
                let tool_path = to_snake_case(name);
                let tool_dep_line = format!("{} = {{ path = \"../tools/{}\" }}\n", tool_path, tool_path);
                if !cargo_toml.contains(&tool_dep_line) {
                    // Find the end of the [dependencies] section
                    if let Some(pos) = cargo_toml.find("\n[dependencies]\n") {
                        let insert_pos = pos + "\n[dependencies]\n".len();
                        cargo_toml.insert_str(insert_pos, &tool_dep_line);
                    } else if let Some(pos) = cargo_toml.find("[dependencies]\n") {
                        let insert_pos = pos + "[dependencies]\n".len();
                        cargo_toml.insert_str(insert_pos, &tool_dep_line);
                    } else {
                        // If no [dependencies] section exists, create one
                        cargo_toml.push_str("\n[dependencies]\n");
                        cargo_toml.push_str(&tool_dep_line);
                    }
                    
                    std::fs::write(&cargo_toml_path, cargo_toml)
                        .expect("Failed to write to Cargo.toml");
                }
                
                // Read and update main.rs
                let main_rs_content = std::fs::read_to_string(&main_rs_path)
                    .expect("Failed to read main.rs");
                
                let import_line = format!("use {}::{};\n", tool_path, name);
                let register_line = format!("    app.add_tools({});\n", name);
                let mut updated_main_rs = main_rs_content.clone();
                
                // Add import if not already present
                if !updated_main_rs.contains(&import_line) {
                    // Insert after the existing use statement
                    if let Some(pos) = updated_main_rs.find("use app::App;") {
                        let insert_pos = pos + "use app::App;".len();
                        updated_main_rs.insert_str(insert_pos, &format!("\n{}", import_line.trim_end()));
                    }
                }
                
                // Add tool registration if not already present
                if !updated_main_rs.contains(&register_line.trim()) {
                    // Insert before the app.run() line
                    if let Some(pos) = updated_main_rs.find("    app.run()") {
                        updated_main_rs.insert_str(pos, &register_line);
                    }
                }
                
                std::fs::write(&main_rs_path, updated_main_rs)
                    .expect("Failed to write to main.rs");
                
                println!("Added {} to agent", name);
            }
            
            ToolCommands::New { name } => {
                if !is_upper_camel_case(name) {
                    eprintln!("Error: Tool name must follow UpperCamelCase convention (e.g., MyToolName)");
                    std::process::exit(1);
                }
                
                let yushi_root = std::env::current_dir().expect("Failed to get current directory");
                let tools_dir = yushi_root.join("tools");
                
                if !tools_dir.exists() {
                    eprintln!("Error: This command can only be run from within a yushi project directory");
                    std::process::exit(1);
                }

                let snake_name = to_snake_case(name);
                let tool_dir = tools_dir.join(&snake_name);
                
                let output = Command::new("cargo")
                    .args(["new", "--lib", &snake_name])
                    .current_dir(&tools_dir)
                    .output()
                    .expect("Failed to execute cargo new command");
                
                if !output.status.success() {
                    eprintln!("Failed to create tool project: {}", String::from_utf8_lossy(&output.stderr));
                    std::process::exit(1);
                }
                
                // Update the tool's Cargo.toml to include jieyusha dependency
                let cargo_toml_path = tool_dir.join("Cargo.toml");
                let mut cargo_toml = std::fs::read_to_string(&cargo_toml_path)
                    .expect("Failed to read tool's Cargo.toml");
                
                cargo_toml.push_str("jieyusha.workspace = true\n");
                cargo_toml.push_str("async-trait.workspace = true\n");
                cargo_toml.push_str("serde.workspace = true\n");
                cargo_toml.push_str("serde_json.workspace = true\n");
                    
                std::fs::write(&cargo_toml_path, cargo_toml)
                        .expect("Failed to write to tool's Cargo.toml");
                
                // Create lib.rs with jieyusha prelude import
                let lib_rs_path = tool_dir.join("src").join("lib.rs");
                std::fs::write(&lib_rs_path, format!(r##"
use async_trait::async_trait;
use jieyusha::*;

/// TODO: Implement your tool here
pub struct {};

// TODO: Implement the Tool trait for your tool
#[async_trait]
impl Tool for {} {{
    fn name(&self) -> &str {{
        "{}"
    }}

    fn description(&self) -> &str {{
        "TODO: Describe what your tool does"
    }}
    
    async fn prompt(&self) -> String {{
        "TODO: Provide a prompt for your tool".to_string()
    }}

    fn input_json_schema(&self) -> &str {{
        r#"{{}}"#
    }}

    async fn call(&self, input: &serde_json::Value, context: &mut ToolUseContext) -> Result<ToolMessage> {{
        todo!("Implement your tool logic here")
    }}
}}
"##, name, name, name))
                .expect("Failed to write to tool's lib.rs");
                
                println!("Successfully created tool: tools/{}", snake_name);
            }
            
            ToolCommands::Delete { name } => {
                if !is_upper_camel_case(name) {
                    eprintln!("Error: Tool name must follow UpperCamelCase convention (e.g., MyToolName)");
                    std::process::exit(1);
                }
                
                let yushi_root = std::env::current_dir().expect("Failed to get current directory");
                let tools_dir = yushi_root.join("tools");
                
                if !tools_dir.exists() {
                    eprintln!("Error: This command can only be run from within a yushi project directory");
                    std::process::exit(1);
                }
                
                let snake_name = to_snake_case(name);
                let tool_dir = tools_dir.join(&snake_name);
                
                if !tool_dir.exists() {
                    eprintln!("Error: Tool '{}' does not exist", name);
                    std::process::exit(1);
                }
                
                if let Err(e) = std::fs::remove_dir_all(&tool_dir) {
                    eprintln!("Error: Failed to remove tool directory '{}': {}", snake_name, e);
                    std::process::exit(1);
                }
                
                // Update the workspace Cargo.toml to remove from members
                let workspace_cargo_toml = yushi_root.join("Cargo.toml");
                if workspace_cargo_toml.exists() {
                    let cargo_toml_content = std::fs::read_to_string(&workspace_cargo_toml)
                        .expect("Failed to read workspace Cargo.toml");
                    
                    let mut lines: Vec<String> = cargo_toml_content.lines().map(|s| s.to_string()).collect();
                    let mut in_members_section = false;
                    let mut i = 0;
                    
                    while i < lines.len() {
                        let line = &lines[i];
                        
                        if line.trim_start().starts_with("[workspace]") {
                            in_members_section = true;
                        } else if in_members_section && line.contains("members") && line.contains("=") {
                            // Handle members array
                            if line.contains("[") && line.contains("]") {
                                // Single line members array
                                let member_entry = format!("\"tools/{}\"", snake_name);
                                if line.contains(&member_entry) {
                                    let new_line = line.replace(&format!("{}, ", &member_entry), "")
                                        .replace(&format!(", {}", &member_entry), "")
                                        .replace(&member_entry, "");
                                    lines[i] = new_line;
                                }
                            } else if line.contains("[") {
                                // Multi-line members array starting
                                let mut j = i + 1;
                                while j < lines.len() && !lines[j].contains("]") {
                                    let member_entry = format!("\"tools/{}\"", snake_name);
                                    if lines[j].contains(&member_entry) {
                                        lines.remove(j);
                                        // Don't increment j since we removed a line
                                    } else {
                                        j += 1;
                                    }
                                }
                            }
                        } else if line.trim_start().starts_with("[") && !line.trim_start().starts_with("[workspace]") {
                            // We've moved to another section
                            in_members_section = false;
                        }
                        
                        i += 1;
                    }
                    
                    let updated_content = lines.join("\n");
                    std::fs::write(&workspace_cargo_toml, updated_content)
                        .expect("Failed to write to workspace Cargo.toml");
                }
                
                println!("Successfully deleted tool: {}", name);
            }
            
            ToolCommands::Remove { name } => {
                if !is_upper_camel_case(name) {
                    eprintln!("Error: Tool name must be in UpperCamelCase");
                    std::process::exit(1);
                } 

                let project_root = std::env::current_dir().expect("Failed to get current directory");
                let cargo_toml_path = project_root.join("Cargo.toml");
                let main_rs_path = project_root.join("src").join("main.rs");
                let yushi_assets_dir = project_root.join(".yushi");
                if !yushi_assets_dir.exists() {
                    eprintln!("Error: This command can only be run from within a agent directory");
                    std::process::exit(1);
                }
                
                // Read and update Cargo.toml to remove the dependency
                let mut cargo_toml = std::fs::read_to_string(&cargo_toml_path)
                    .expect("Failed to read Cargo.toml");
                
                let tool_path = to_snake_case(name);
                let tool_dep_line = format!("{} = {{ path = \"../tools/{}\" }}\n", tool_path, tool_path);
                
                if cargo_toml.contains(&tool_dep_line) {
                    cargo_toml = cargo_toml.replace(&tool_dep_line, "");
                    std::fs::write(&cargo_toml_path, cargo_toml)
                        .expect("Failed to write to Cargo.toml");
                }
                
                // Read and update main.rs to remove import and registration
                let main_rs_content = std::fs::read_to_string(&main_rs_path)
                    .expect("Failed to read main.rs");
                
                let import_line = format!("use {}::{};\n", tool_path, name);
                let register_line = format!("    app.add_tools({});\n", name);
                let mut updated_main_rs = main_rs_content.clone();
                
                // Remove import if present
                if updated_main_rs.contains(&import_line) {
                    updated_main_rs = updated_main_rs.replace(&import_line, "");
                }
                
                // Remove tool registration if present
                if updated_main_rs.contains(&register_line.trim()) {
                    updated_main_rs = updated_main_rs.replace(&register_line, "");
                }
                
                std::fs::write(&main_rs_path, updated_main_rs)
                    .expect("Failed to write to main.rs");
                
                println!("Removed {} from agent", name);
            }
        }

        Some(Commands::Delete { name }) => {
            // Check if we are in a yushi project directory by looking for a workspace Cargo.toml
            let workspace_cargo_toml = PathBuf::from("Cargo.toml");
            if !workspace_cargo_toml.exists() {
                eprintln!("Error: No Cargo.toml found in current directory. Please run this command from the root of a yushi project.");
                std::process::exit(1);
            }

            // Verify this is a workspace Cargo.toml by checking for [workspace] section
            let cargo_toml_content = std::fs::read_to_string(&workspace_cargo_toml)
                .expect("Failed to read Cargo.toml");
            
            if !cargo_toml_content.contains("[workspace]") {
                eprintln!("Error: This doesn't appear to be a yushi workspace. Missing [workspace] section in Cargo.toml.");
                std::process::exit(1);
            }

            let project_path = PathBuf::from(name);
            
            if !project_path.exists() {
                eprintln!("Error: Project directory '{}' does not exist", name);
                std::process::exit(1);
            }
            
            if let Err(e) = std::fs::remove_dir_all(&project_path) {
                eprintln!("Error: Failed to remove project directory '{}': {}", name, e);
                std::process::exit(1);
            }
            
            // Update workspace Cargo.toml to remove from members
            if workspace_cargo_toml.exists() {
                let cargo_toml_content = std::fs::read_to_string(&workspace_cargo_toml)
                    .expect("Failed to read workspace Cargo.toml");
                
                let mut lines: Vec<String> = cargo_toml_content.lines().map(|s| s.to_string()).collect();
                let mut in_members_section = false;
                let mut i = 0;
                
                while i < lines.len() {
                    let line = &lines[i];
                    
                    if line.trim_start().starts_with("[workspace]") {
                        in_members_section = true;
                    } else if in_members_section && line.contains("members") && line.contains("=") {
                        // Handle members array
                        if line.contains("[") && line.contains("]") {
                            // Single line members array
                            let member_entry = format!("\"{}\"", name);
                            if line.contains(&member_entry) {
                                let new_line = line.replace(&format!("{}, ", &member_entry), "")
                                    .replace(&format!(", {}", &member_entry), "")
                                    .replace(&member_entry, "");
                                lines[i] = new_line;
                            }
                        } else if line.contains("[") {
                            // Multi-line members array starting
                            let mut j = i + 1;
                            while j < lines.len() && !lines[j].contains("]") {
                                let member_entry = format!("\"{}\"", name);
                                if lines[j].contains(&member_entry) {
                                    lines.remove(j);
                                    // Don't increment j since we removed a line
                                } else {
                                    j += 1;
                                }
                            }
                        }
                    } else if line.trim_start().starts_with("[") && !line.trim_start().starts_with("[workspace]") {
                        // We've moved to another section
                        in_members_section = false;
                    }
                    
                    i += 1;
                }
                
                let updated_content = lines.join("\n");
                std::fs::write(&workspace_cargo_toml, updated_content)
                    .expect("Failed to write to workspace Cargo.toml");
            }
            
            println!("Successfully deleted agent '{}'", name);
        }

        None => {}
    }
}

fn is_upper_camel_case(s: &str) -> bool {
    if s.is_empty() || !s.chars().next().unwrap().is_ascii_uppercase() {
        return false;
    }
    
    s.chars().all(|c| c.is_ascii_alphanumeric()) && 
    !s.contains("_") && 
    !s.chars().any(|c| c.is_ascii_lowercase() && c.is_ascii_digit())
}

// Helper function to convert UpperCamelCase to kebab-case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}
