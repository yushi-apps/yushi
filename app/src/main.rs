use std::io::{self, Write};
use std::fs::{self, OpenOptions};
use std::borrow::Cow;
use std::sync::Arc;

use reedline::{Reedline, Signal, FileBackedHistory, History};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};
use reedline::HistoryItem;

mod app;
use app::App;
use bash::BashTool;

use automation::executor::Executor;
use automation::engine::{RuleCommand, RuleEngine};
use automation::tool::RuleTool;
use automation::storage::Storage;

enum Mode {
	Dialog,
	Memory,
	Slash,
	Multiline,
}

fn read_line(_prompt: &str) -> io::Result<String> {
    let mut rl = Reedline::create();
    if let Some(history) = get_history() {
        rl = rl.with_history(Box::new(history));
    };
	
	let prompt = CustomPrompt;

    let out = rl.read_line(&prompt);
    match out {
        Ok(Signal::Success(buffer)) => {
            Ok(buffer)
        }
        Ok(Signal::CtrlC) => {
            println!("^C");
            std::process::exit(0);
        }
        Ok(Signal::CtrlD) => {
            // Handle ^D as EOF
            Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"))
        }
        Err(e) => Err(e),
    }
}

fn read_multiline_prefilled(initial: Option<String>) -> io::Result<String> {
    let mut result = String::new();

    let process_line = |mut line: String, accum: &mut String| -> bool {
        if line.ends_with('\\') {
            // remove trailing backslash and append without adding a newline
            line.pop();
            accum.push_str(&line);
            true 
        } else {
            accum.push_str(&line);
            false 
        }
    };

    if let Some(init) = initial {
        if !init.is_empty() {
            let cont = process_line(init, &mut result);
            if !cont {
                return Ok(result);
            }
        }
    }

    let mut rl = Reedline::create();
	let prompt = CustomPrompt;

    loop {
        let out = rl.read_line(&prompt);
        match out {
            Ok(Signal::Success(buf)) => {
                let line = buf.trim_end_matches(&['\r','\n'][..]).to_string();
                let cont = process_line(line, &mut result);
                if !cont {
                    break;
                }
            }
            Ok(Signal::CtrlC) => {
                println!("^C");
                std::process::exit(0);
            }
            Ok(Signal::CtrlD) => {
                // Handle ^D as EOF
                break; 
            }
            Err(e) => return Err(e),
        }
    }
    Ok(result)
}

fn append_to_agents(content: &str) -> io::Result<()> {
	let path = App::agents_md();
	let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
	writeln!(file, "{}", content)?;
	println!("Appended to {}", path.display());
	Ok(())
}

fn handle_slash_command(app: &App, cmd: &str) -> bool {
	let args = cmd.trim();
	if args.is_empty() {
		println!("Slash mode: enter a command (help to list).");
		return false;
	}
	let mut parts = args.split_whitespace();
	match parts.next().unwrap_or("") {
		"help" => {
			println!("Built-in commands:");
			println!("/help    - this help");
			println!("/pin     - Pin the current conversation to a new agent file");
			println!("/agents  - Subagent commands: list, create <name>, delete <name>");
			println!("/config  - Show system configuration of Yushi CLI");
			println!("/clear   - Clear the history");
			println!("/quit    - Exit TUI");
		}
		"pin" => {
			match read_line("Agent name: ") {
				Ok(name) if !name.trim().is_empty() => {
					let dir = App::agents_dir();
					if let Err(e) = fs::create_dir_all(&dir) {
						eprintln!("Failed to create agents dir {}: {}", dir.display(), e);
					} else {
						let file_path = dir.join(format!("{}.md", name.trim()));
						// Note: Reedline history doesn't provide access to full history,
						// so we can't save it to the agent file anymore
						// We'll just create an empty agent file instead
						let content = format!("# Agent: {}\n\n", name.trim());
						if let Err(e) = fs::write(&file_path, content) {
							eprintln!("Failed to write agent file {}: {}", file_path.display(), e);
						} else {
							println!("Pinned conversation to {}", file_path.display());
						}
					}
				}
				Ok(_) => println!("Agent name empty, aborting pin."),
				Err(e) => eprintln!("I/O error: {}", e),
			}
		}
		"agents" => {
			match read_line("agents> ") {
				Ok(line) => {
					let mut sp = line.split_whitespace();
					match sp.next() {
						Some("list") => {
							let dir = App::agents_dir();
							match fs::read_dir(&dir) {
								Ok(rd) => {
									println!("Subagents in {}:", dir.display());
									for e in rd.flatten() {
										if let Some(n) = e.path().file_name().and_then(|s| s.to_str()) {
											println!("- {}", n);
										}
									}
								}
								Err(_) => println!("No agents directory found ({}).", dir.display()),
							}
						}
						Some("create") => {
							if let Some(name) = sp.next() {
								let dir = App::agents_dir();
								if let Err(e) = fs::create_dir_all(&dir) {
									eprintln!("Failed to create agents dir: {}", e);
								} else {
									let file = dir.join(format!("{}.md", name));
									if fs::write(&file, format!("# Agent: {}\n\n", name)).is_ok() {
										println!("Created {}", file.display());
									} else {
										eprintln!("Failed to create {}", file.display());
									}
								}
							} else {
								println!("Usage: create <name>");
							}
						}
						Some("delete") => {
							if let Some(name) = sp.next() {
								let file = App::agents_dir().join(format!("{}.md", name));
								match fs::remove_file(&file) {
									Ok(_) => println!("Deleted {}", file.display()),
									Err(_) => println!("Failed to delete {} (not found?).", file.display()),
								}
							} else {
								println!("Usage: delete <name>");
							}
						}
						_ => {
							println!("agents subcommands: list | create <name> | delete <name>");
						}
					}
				}
				Err(e) => eprintln!("I/O error: {}", e),
			}
		}
		"config" => {
			let repo_root = App::root_path();
			println!("Yushi CLI config:");
			println!("- Runtime: tokio");
			println!("- Install root: {}", repo_root.display());
			println!("- AGENTS.md: {}", App::agents_md().display());
			println!("- Agents dir: {}", App::agents_dir().display());
			println!("- History dir: {}", App::history_dir().display());
		}
		"clear" => {
			// Clearing Reedline history
			if let Some(mut history) = get_history() {
				history.clear().unwrap_or_else(|e| eprintln!("Failed to clear history: {}", e));
			}
			println!("Session history cleared.");
		}
		"quit" | "exit" => {
			return true;
		}
		other => {
			println!("Unknown command: {}", other);
			println!("Use /help for available commands.");
		}
	}
	false
}

async fn run(app: &App) -> io::Result<()> {
    let mut mode = Mode::Dialog;
    println!("Yushi CLI - History: {}", App::history_dir().display());
    println!("Type / for commands, # to append memory, \\ for multiline. Ctrl+C to quit.");
    loop {
        match mode {
            Mode::Dialog => {
                let line = read_line("> ")?;
                if line.starts_with('/') {
                    if line.trim() == "/" {
                        mode = Mode::Slash;
                        continue;
                    }
                    let cmd_inline = &line[1..];
                    let should_exit = handle_slash_command(app, cmd_inline);
                    if should_exit {
                        break;
                    }
                    continue;
                }

                if line.starts_with('#') {
                    if line.trim() == "#" {
                        mode = Mode::Memory;
                        continue;
                    } else {
                        let content = line[1..].trim_start();
                        if !content.is_empty() {
                            if let Err(e) = append_to_agents(content) {
                                eprintln!("Failed to append memory: {}", e);
                            }
                        } else {
                            println!("No content after '#', nothing appended.");
                        }
                        continue;
                    }
                }

                if line.ends_with('\\') {
                    if line.trim() == "\\" {
                        mode = Mode::Multiline;
                        continue;
                    } else {
                        let content = read_multiline_prefilled(Some(line))?;
                        if !content.trim().is_empty() {
                            add_to_history(&content);
                            println!("❉ Working...");
                            let response = jieyusha::chat(&content, "main").await.unwrap_or_default();
                            print_formatted_response(&response);
                            add_to_history(&response);
                        } else {
                            println!("Empty multiline input; ignored.");
                        }
                        continue;
                    }
                }

                if line == "#" {
                    mode = Mode::Memory;
                    continue;
                } else if line == "/" {
                    println!("Going to Slash mode. Enter commands starting with /");
                    mode = Mode::Slash;
                    continue;
                } else if line == "\\" {
                    mode = Mode::Multiline;
                    continue;
                } else if line.trim().is_empty() {
                    continue;
                } else {
                    add_to_history(&line);

                    println!("❉ Thinking...");
                    let response = jieyusha::chat(&line, "main").await.unwrap_or_default();
                    print_formatted_response(&response);
                }
            }
            Mode::Memory => {
                // use prefilling API with None (unchanged behavior)
                let content = read_multiline_prefilled(None)?;
                if !content.trim().is_empty() {
                    if let Err(e) = append_to_agents(&content) {
                        eprintln!("Failed to append memory: {}", e);
                    }
                } else {
                    println!("No content; nothing appended.");
                }
                mode = Mode::Dialog;
            }
            Mode::Slash => {
                let cmd = read_line("/ ")?;
                // run command; if command signals exit, break
                let should_exit = handle_slash_command(app, &cmd);
                if should_exit {
                    break;
                }
                mode = Mode::Dialog;
            }
            Mode::Multiline => {
                let content = read_multiline_prefilled(None)?;
                if !content.trim().is_empty() {
                    println!("• Working...");
                    let response = jieyusha::chat(&content, "main").await.unwrap_or_default();
                    print_formatted_response(&response);
                    add_to_history(&content);
                    add_to_history(&response);

                } else {
                    println!("Empty multiline input; ignored.");
                }
                mode = Mode::Dialog;
            }
        }
    }
    Ok(())
}

// update main to use tokio runtime
#[tokio::main]
async fn main() {
    // 确保 history 目录存在
    let history_dir = App::history_dir();
    if !history_dir.exists() {
        fs::create_dir_all(&history_dir).expect("Failed to create history directory");
    }
    
    let mut app = App::new();
    app.trace("DEBUG");

    // ==== Automation 初始化 ====
    // 1. 获取 automation 根目录
    let root = App::root_path();
    let automation_dir = root.join("automation");

    // 2. 创建 Storage 并初始化
    let storage = Arc::new(Storage::new(automation_dir.clone()));
    storage.ensure_dirs().expect("Failed to create automation dirs");
    storage.init_base_files().expect("Failed to init base files");

    // 3. 创建 Executor
    let executor = Arc::new(Executor::new(automation_dir.clone()));

    // 4. 加载已有规则和事件源
    let rules = storage.load_merged_rules().unwrap_or_default();
    let sources = storage.load_merged_sources().unwrap_or_default();

    // 5. 启动 RuleEngine
    let (cmd_tx, _event_tx, engine_handle) = RuleEngine::start(executor.clone(), rules, sources);

    // 6. 注册 RuleTool (clone cmd_tx，因为关机时还需要使用)
    let shutdown_tx = cmd_tx.clone();
    app.add_tools(RuleTool::new(cmd_tx, storage));

    app.add_tools(BashTool);
    if let Err(e) = run(&app).await {
        eprintln!("Error: {}", e);
    }

    // 优雅关闭 RuleEngine
    let _ = shutdown_tx.send(RuleCommand::Shutdown).await;
    let _ = engine_handle.await;
}

struct CustomPrompt;

impl Prompt for CustomPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
		Cow::Borrowed("Yushi> ")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
		Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<'_, str> {
		Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_history_search_indicator(&self, _history_search: PromptHistorySearch) -> Cow<'_, str> {
		Cow::Borrowed("")
    }
}

fn print_formatted_response(response: &str) {
    let lines: Vec<&str> = response.lines().collect();
    if let Some(first_line) = lines.first() {
        println!("● {}", first_line);
    }
    for line in lines.iter().skip(1) {
        println!("  {}", line);
    }
}

fn get_history() -> Option<FileBackedHistory> {
	let path = App::history_dir().join("cli.txt");
	FileBackedHistory::with_file(1000, path).ok()
}

fn add_to_history(content: &str) {
    if let Some(mut history) = get_history() {
        if let Err(e) = history.save(HistoryItem::from_command_line(content)) {
            eprintln!("Failed to save to history: {}", e);
        }
    }
}

fn show_progress_message(message: &jieyusha::messages::ProgressMessage) {
    println!("{}", message.content.content);
}