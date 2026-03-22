//! OpenAI-compatible LLM implementation

use crate::providers::error::ProviderError;
use crate::providers::openai_compat::OpenAICompatProvider;
use crate::providers::traits::{LlmConfig, StreamCallback};
use crate::providers::types::{
    CompletionResponse, Message, StreamDelta, ToolCall, ToolCallFunction,
    ToolDefinition,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

// ==================== Request Types ====================

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    stream: bool,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ApiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct ApiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: ApiFunctionDef,
}

#[derive(Serialize)]
struct ApiFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct ApiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: ApiFunctionCall,
}

#[derive(Serialize, Clone)]
struct ApiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    json_schema: Option<JsonSchemaWrapper>,
}

#[derive(Serialize)]
struct JsonSchemaWrapper {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}

// ==================== Response Types ====================

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ResponseToolCall>>,
}

#[derive(Deserialize, Clone)]
struct ResponseToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: ResponseFunctionCall,
}

#[derive(Deserialize, Clone)]
struct ResponseFunctionCall {
    name: String,
    arguments: String,
}

// ==================== Streaming Types ====================

#[derive(Deserialize)]
struct StreamingResponse {
    choices: Vec<StreamingChoice>,
}

#[derive(Deserialize)]
struct StreamingChoice {
    delta: StreamingDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamingDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamingToolCall>>,
}

#[derive(Deserialize)]
struct StreamingToolCall {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: Option<StreamingFunction>,
}

#[derive(Deserialize)]
struct StreamingFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Default, Clone)]
struct ToolCallAccumulator {
    id: String,
    call_type: String,
    name: String,
    arguments: String,
}

// ==================== Conversion Functions ====================

fn convert_message(msg: &Message) -> ApiMessage {
    ApiMessage {
        role: msg.role.as_str().to_string(),
        content: msg.content.clone(),
        tool_calls: msg.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| ApiToolCall {
                    id: tc.id.clone(),
                    call_type: tc.call_type.clone().unwrap_or_else(|| "function".to_string()),
                    function: ApiFunctionCall {
                        name: tc.get_name().unwrap_or_default().to_string(),
                        arguments: tc.get_arguments().unwrap_or_default().to_string(),
                    },
                })
                .collect()
        }),
        tool_call_id: msg.tool_call_id.clone(),
        name: msg.name.clone(),
    }
}

fn convert_tool(tool: &ToolDefinition) -> ApiTool {
    ApiTool {
        tool_type: "function".to_string(),
        function: ApiFunctionDef {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
    }
}

fn convert_tool_call(tc: &ResponseToolCall) -> ToolCall {
    ToolCall {
        id: tc.id.clone(),
        call_type: Some(tc.call_type.clone()),
        function: Some(ToolCallFunction {
            name: tc.function.name.clone(),
            arguments: tc.function.arguments.clone(),
        }),
        name: None,
        arguments: None,
    }
}

// ==================== Non-Streaming Implementation ====================

pub async fn complete(
    provider: &OpenAICompatProvider,
    messages: &[Message],
    config: &LlmConfig,
) -> Result<CompletionResponse, ProviderError> {
    complete_internal(provider, messages, &[], config).await
}

pub async fn complete_with_tools(
    provider: &OpenAICompatProvider,
    messages: &[Message],
    tools: &[ToolDefinition],
    config: &LlmConfig,
) -> Result<CompletionResponse, ProviderError> {
    complete_internal(provider, messages, tools, config).await
}

async fn complete_internal(
    provider: &OpenAICompatProvider,
    messages: &[Message],
    tools: &[ToolDefinition],
    config: &LlmConfig,
) -> Result<CompletionResponse, ProviderError> {
    let api_messages: Vec<ApiMessage> = messages.iter().map(convert_message).collect();
    let api_tools: Option<Vec<ApiTool>> = if tools.is_empty() {
        None
    } else {
        Some(tools.iter().map(convert_tool).collect())
    };

    let response_format = config.params.structured_output.as_ref().map(|schema| {
        ResponseFormat {
            format_type: "json_schema".to_string(),
            json_schema: Some(JsonSchemaWrapper {
                name: schema.name.clone(),
                strict: schema.strict,
                schema: schema.schema.clone(),
            }),
        }
    });

    let request = ChatRequest {
        model: config.model.clone(),
        messages: api_messages,
        tools: api_tools,
        tool_choice: None,
        temperature: config.params.temperature,
        max_tokens: config.params.max_tokens,
        response_format,
        stream: false,
    };

    let mut req = provider
        .client()
        .post(format!("{}/chat/completions", provider.base_url()))
        .header("Content-Type", "application/json");

    if let Some(api_key) = provider.api_key() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = req.json(&request).send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        if status == 429 {
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        return Err(ProviderError::Api {
            status,
            message: body,
        });
    }

    let body = response.text().await?;

    let chat_response: ChatResponse = serde_json::from_str(&body)
        .map_err(|e| {
            eprintln!("OpenAI-compat LLM parse error: {e}");
            eprintln!("Response body (first 500 chars): {}", &body[..body.len().min(500)]);
            ProviderError::ParseError(format!("Failed to parse chat response: {e}"))
        })?;

    let choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::ParseError("No choices in response".to_string()))?;

    let tool_calls = choice
        .message
        .tool_calls
        .map(|tcs| tcs.iter().map(convert_tool_call).collect());

    Ok(CompletionResponse {
        content: choice.message.content.unwrap_or_default(),
        tool_calls,
    })
}

// ==================== Streaming Implementation ====================

pub async fn complete_streaming_with_tools(
    provider: &OpenAICompatProvider,
    messages: &[Message],
    tools: &[ToolDefinition],
    config: &LlmConfig,
    on_delta: StreamCallback,
) -> Result<CompletionResponse, ProviderError> {
    let api_messages: Vec<ApiMessage> = messages.iter().map(convert_message).collect();
    let api_tools: Option<Vec<ApiTool>> = if tools.is_empty() {
        None
    } else {
        Some(tools.iter().map(convert_tool).collect())
    };

    let request = ChatRequest {
        model: config.model.clone(),
        messages: api_messages,
        tools: api_tools,
        tool_choice: None,
        temperature: config.params.temperature,
        max_tokens: config.params.max_tokens,
        response_format: None,
        stream: true,
    };

    let mut req = provider
        .client()
        .post(format!("{}/chat/completions", provider.base_url()))
        .header("Content-Type", "application/json");

    if let Some(api_key) = provider.api_key() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = req.json(&request).send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        if status == 429 {
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        return Err(ProviderError::Api {
            status,
            message: body,
        });
    }

    let mut content = String::new();
    let mut tool_call_accumulators: Vec<ToolCallAccumulator> = Vec::new();
    let mut buffer = String::new();
    let mut finish_reason = None;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| ProviderError::Network(e.to_string()))?;
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if line == "data: [DONE]" {
                on_delta(StreamDelta::Done {
                    finish_reason: finish_reason.clone(),
                });
                break;
            }

            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(response) = serde_json::from_str::<StreamingResponse>(json_str) {
                    if let Some(choice) = response.choices.first() {
                        if choice.finish_reason.is_some() {
                            finish_reason = choice.finish_reason.clone();
                        }

                        if let Some(delta_content) = &choice.delta.content {
                            content.push_str(delta_content);
                            on_delta(StreamDelta::Content(delta_content.clone()));
                        }

                        if let Some(tool_calls) = &choice.delta.tool_calls {
                            for tc in tool_calls {
                                while tool_call_accumulators.len() <= tc.index {
                                    tool_call_accumulators.push(ToolCallAccumulator::default());
                                }

                                let acc = &mut tool_call_accumulators[tc.index];
                                let mut name_changed = false;

                                if let Some(id) = &tc.id {
                                    acc.id = id.clone();
                                }
                                if let Some(call_type) = &tc.call_type {
                                    acc.call_type = call_type.clone();
                                }
                                if let Some(func) = &tc.function {
                                    if let Some(name) = &func.name {
                                        if acc.name.is_empty() {
                                            acc.name = name.clone();
                                            name_changed = true;
                                        }
                                    }
                                    if let Some(args) = &func.arguments {
                                        acc.arguments.push_str(args);
                                        on_delta(StreamDelta::ToolCallArguments {
                                            index: tc.index,
                                            arguments: args.clone(),
                                        });
                                    }
                                }

                                if name_changed && !acc.id.is_empty() && !acc.name.is_empty() {
                                    on_delta(StreamDelta::ToolCallStart {
                                        index: tc.index,
                                        id: acc.id.clone(),
                                        name: acc.name.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let tool_calls = if tool_call_accumulators.is_empty() {
        None
    } else {
        Some(
            tool_call_accumulators
                .into_iter()
                .map(|acc| ToolCall {
                    id: acc.id,
                    call_type: Some(acc.call_type),
                    function: Some(ToolCallFunction {
                        name: acc.name,
                        arguments: acc.arguments,
                    }),
                    name: None,
                    arguments: None,
                })
                .collect(),
        )
    };

    Ok(CompletionResponse {
        content,
        tool_calls,
    })
}
