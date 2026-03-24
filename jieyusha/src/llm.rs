use serde::{Serialize, Deserialize};
use async_trait::async_trait;
use crate::error::{Result, JieyushaError};
use crate::messages::{Message, ChatMessage, AssistantMessage, UnifiedRequest, ToolUse};

pub enum LlmApiType {
    ChatCompletions,     // OpenAI compatible
    Responses           // GPT-5
}

impl LlmApiType {
    pub fn determine_api_type(model: &ModelProfile) -> LlmApiType {
        if model.base_url.contains("chat/completions") {
            return LlmApiType::ChatCompletions;
        } else {
            return LlmApiType::Responses;

        }
    }
}

//pub struct Modelcapabilities {
//    pub api_type: LlmApiType,
//}
//
//const CHAT_COMPLETIONS_CAPABILITIES: Modelcapabilities = Modelcapabilities { api_type: LlmApiType::ChatCompletions};

#[async_trait]
pub trait LlmProvider {
    async fn request(req: UnifiedRequest) -> Result<AssistantMessage>;
}

pub struct ChatCompletionsProvider;

#[async_trait]
impl LlmProvider for ChatCompletionsProvider {
    async fn request(req: UnifiedRequest) -> Result<AssistantMessage> {
        let mut total_messages = vec![serde_json::json!({
            "role": "system",
            "content": req.system_prompt.join("\n")
        })];

        // Prepare messages for LLM
        for message in req.messages {
            match message {
                Message::User(user_message) => {
                    total_messages.push(serde_json::json!({
                        "role": user_message.message.role,
                        "content": user_message.message.content,
                    }));
                },

                Message::Assistant(assistant_message) => {
                    let assistant_json = if let Some(tool_uses) = &assistant_message.tool_uses {
                        //let mut tool_calls = Vec::new();

                        let tool_calls: Vec<serde_json::Value> = tool_uses
                            .iter()
                            .map(|tool_use| {
                                serde_json::json!({
                                    "id": tool_use.id,
                                    "type": "function",
                                    "function": {
                                        "name": tool_use.name,
                                        "arguments": tool_use.arguments.clone(),
                                    }
                                })
                            })
                            .collect();

                        serde_json::json!({
                            "role": "assistant",
                            "content": assistant_message.content,
                            "tool_calls": tool_calls
                        })
                    } else {
                        serde_json::json!({
                            "role": "assistant",
                            "content": assistant_message.content,
                        })
                    };


                    total_messages.push(assistant_json);
                },

                Message::Tool(tool_message) => {
                    total_messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_message.tool_use_id,
                        "content": tool_message.content,
                    }));
                },
                Message::Progress(_) => {}
            }
        }

        // Prepare tools for LLM
        let input_tools = match req.tools {
            Some(tools) => {
                let input_tools: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|tool| {
                        let parameters: serde_json::Value = serde_json::from_str(&tool.input_json_schema())
                            .unwrap_or_else(|e| {
                                log::warn!("Warning: Failed to parse input json schema for {}: {}", tool.name(), e);
                                serde_json::json!({})
                            });

                        serde_json::json!({
                            "type": "function",
                            "function": {
                                "name": tool.name(),
                                "description": tool.description(),
                                "parameters": parameters, 
                            }
                        })
                    }).collect();

                Some(input_tools)
            },

            None => None,
        }; 

        let request_body = serde_json::json!({
            "messages": total_messages,
            "model": req.model.model_name,
            "frequency_penalty": 0,
            "max_tokens": req.model.max_tokens,
            "presence_penalty": 0,
            "response_format": {"type": "text"},
            "stop": null,
            "stream": false,
            "temperature": req.model.temperature,
            "top_p": 1,
            "tools": input_tools,
            //"logprobs": false,
            //"top_logprobs": null,
        });

        log::debug!("LLM Request Body: {:?}\n", request_body);
        
        // 脱敏 API Key 显示
        let masked_api_key = if req.model.api_key.len() > 8 {
            format!("{}****{}", &req.model.api_key[..4], &req.model.api_key[req.model.api_key.len()-4..])
        } else {
            "****".to_string()
        };
        
        log::info!(
            "LLM Request: POST {} (model: {}, api_key: {})",
            req.model.base_url,
            req.model.model_name,
            masked_api_key
        );
        
        let request_start = std::time::Instant::now();
        let request = reqwest::Client::new()
            .post(req.model.base_url.clone())
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", req.model.api_key))
            .json(&request_body);

        let response = match request.send().await {
            Ok(resp) => {
                log::debug!("LLM Response: HTTP {} in {:.2}s", resp.status(), request_start.elapsed().as_secs_f64());
                resp
            },
            Err(e) => {
                let error_type = classify_reqwest_error(&e);
                log::error!(
                    "LLM Request Failed: POST {}\n  Error Type: {}\n  Error Detail: {}\n  Duration: {:.2}s",
                    req.model.base_url,
                    error_type,
                    e,
                    request_start.elapsed().as_secs_f64()
                );
                return Err(JieyushaError::NetworkError(e));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "<unable to read body>".to_string());
            log::error!(
                "LLM Response Error: HTTP {}\n  Response Body: {}",
                status,
                error_body
            );
            return Err(JieyushaError::LlmError(format!("HTTP status: {}", status)));
        }

        // 获取 Content-Length 用于诊断
        let content_length = response.headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());
        
        let read_start = std::time::Instant::now();
        let bytes = match response.bytes().await {
            Ok(b) => {
                log::debug!(
                    "LLM Response Body: {} bytes received in {:.2}s (expected: {})",
                    b.len(),
                    read_start.elapsed().as_secs_f64(),
                    content_length.map(|l| l.to_string()).unwrap_or_else(|| "unknown".to_string())
                );
                b
            },
            Err(e) => {
                let error_type = classify_reqwest_error(&e);
                log::error!(
                    "LLM Response Read Failed:\n  Error Type: {}\n  Error Detail: {}\n  Expected Size: {}\n  Duration: {:.2}s",
                    error_type,
                    e,
                    content_length.map(|l| l.to_string()).unwrap_or_else(|| "unknown".to_string()),
                    read_start.elapsed().as_secs_f64()
                );
                return Err(JieyushaError::NetworkError(e));
            }
        };
        
        log::debug!("LLM Response Raw: {:?}", String::from_utf8_lossy(&bytes));

        let llm_response: ChatCompletionResponse = match serde_json::from_slice(&bytes) {
            Ok(resp) => resp,
            Err(e) => {
                // 截断显示原始响应，避免日志过大
                let raw_preview = String::from_utf8_lossy(&bytes);
                let preview = if raw_preview.len() > 500 {
                    format!("{}... (truncated, total {} bytes)", &raw_preview[..500], bytes.len())
                } else {
                    raw_preview.to_string()
                };
                log::error!(
                    "LLM Response Decode Failed:\n  Parse Error: {}\n  Response Preview: {}",
                    e,
                    preview
                );
                return Err(JieyushaError::SerializationError(e));
            }
        };
        let chat_message = match llm_response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message) {
                Some(message) => message,
                None => {
                    return Err(JieyushaError::LlmError("No response choices received".to_string()));
                }
            };

        let tool_uses = match chat_message.tool_calls {
            Some(tool_calls) => {
                let tool_uses: Vec<ToolUse> = tool_calls
                    .into_iter()
                    .map(|tool_call| ToolUse {
                        id: tool_call.id,
                        name: tool_call.function.name,
                        arguments: tool_call.function.arguments,
                    })
                    .collect();
                Some(tool_uses)
            },

            None => None
        }; 
        Ok(AssistantMessage {
            content: chat_message.content,
            uuid: llm_response.id,
            tool_uses: tool_uses,
            //duration_ms: 0,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ModelProfile {
    pub model_name: String,
    pub base_url: String,
    pub api_key: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl ModelProfile { 
    pub fn profile() -> ModelProfileBuilder {
        ModelProfileBuilder::default()
    }
}

#[derive(Default)]
pub struct ModelProfileBuilder {
    model_name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

impl ModelProfileBuilder {
    pub fn model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn build(self) -> ModelProfile {
        ModelProfile {
            model_name: self.model_name.unwrap_or("deepseek-chat".to_string()),
            base_url: self.base_url.unwrap_or("https://api.deepseek.com/chat/completions".to_string()),
            api_key: self.api_key.expect("api_key is reqeuired"),
            max_tokens: self.max_tokens.unwrap_or(4096),
            temperature: self.temperature.unwrap_or(1.0),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub completion_tokens: u32,
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub created: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    pub object: String,
    pub usage: Usage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    pub index: u32,
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Logprobs>
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    InsufficientSystemResources,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Logprobs {
    pub content: Option<Vec<TokenLogprob>>,
    pub reasoning_content: Option<Vec<TokenLogprob>>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenLogprob {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
    pub top_logprobs: Vec<TopLogprobs>
}

 #[derive(Debug, Serialize, Deserialize)]
pub struct TopLogprobs {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
}

/// 分类 reqwest 错误类型，用于日志诊断
fn classify_reqwest_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        if error.is_connect() {
            "CONNECTION_TIMEOUT"
        } else {
            "REQUEST_TIMEOUT"
        }
    } else if error.is_connect() {
        "CONNECTION_FAILED"
    } else if error.is_request() {
        "REQUEST_ERROR"
    } else if error.is_body() {
        "BODY_ERROR"
    } else if error.is_decode() {
        "DECODE_ERROR"
    } else if error.is_redirect() {
        "REDIRECT_ERROR"
    } else if error.is_status() {
        "HTTP_STATUS_ERROR"
    } else {
        "UNKNOWN_ERROR"
    }
}

