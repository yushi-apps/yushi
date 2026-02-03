use serde::{Serialize, Deserialize};
use async_trait::async_trait;
use crate::Registry;
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
    fn as_any(&self) -> &dyn std::any::Any;
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
                        //for tool_use in tool_uses {
                        //    if let Some(tool) = Registry::instance().get_tool(&tool_use.name) {
                        //        let prompt = tool.prompt().await;
                        //        tool_calls.push(serde_json::json!({
                        //            "id": tool_use.id,
                        //            "type": "function",
                        //            "function": {
                        //                "name": tool_use.name,
                        //                //"description": prompt,
                        //                "arguments": tool_use.arguments.clone(),
                        //            }
                        //        }));
                        //    }
                        //}

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
                        "content": serde_json::Value::String(tool_message.content),
                        "tool_call_id": tool_message.tool_use_id,
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
        let request = reqwest::Client::new()
            .post(req.model.base_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", req.model.api_key))
            .json(&request_body);


        log::debug!("LLM Request: {:?}", request);
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(JieyushaError::LlmError(format!("HTTP status: {}", response.status())));
        }

        //log::debug!("LLM Response Raw: {:?}", response);

        let bytes = response.bytes().await?;
        log::debug!("LLM Response Raw: {:?}", String::from_utf8_lossy(&bytes));

        //let llm_response: ChatCompletionResponse = response.json().await?;
        let llm_response: ChatCompletionResponse = serde_json::from_slice(&bytes)?;
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
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

 
