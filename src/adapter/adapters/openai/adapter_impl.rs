use crate::adapter::openai::OpenAIStreamer;
use crate::adapter::support::get_api_key_resolver;
use crate::adapter::{Adapter, AdapterConfig, AdapterKind, ServiceType, WebRequestData};
use crate::chat::{
	ChatMessage, ChatOptionsSet, ChatRequest, ChatResponse, ChatResponsePayload, ChatStream, ChatStreamResponse, MessageContent, MessageExtra, MetaUsage
};

use crate::chat::tool::AssistantToolCall;
use crate::support::value_ext::ValueExt;
use crate::webc::WebResponse;
use crate::{ConfigSet, ModelInfo};
use crate::{Error, Result};
use reqwest::RequestBuilder;
use reqwest_eventsource::EventSource;
use serde_json::{json, Value};
use std::sync::OnceLock;
use tracing::debug;

pub struct OpenAIAdapter;

const BASE_URL: &str = "https://api.openai.com/v1/";
const MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-4", "gpt-3.5-turbo"];

impl Adapter for OpenAIAdapter {
	/// Note: For now returns the common ones (see above)
	async fn all_model_names(_kind: AdapterKind) -> Result<Vec<String>> {
		Ok(MODELS.iter().map(|s| s.to_string()).collect())
	}

	fn default_adapter_config(_kind: AdapterKind) -> &'static AdapterConfig {
		static INSTANCE: OnceLock<AdapterConfig> = OnceLock::new();
		INSTANCE.get_or_init(|| AdapterConfig::default().with_auth_env_name("OPENAI_API_KEY"))
	}

	fn get_service_url(model_info: ModelInfo, service_type: ServiceType) -> String {
		Self::util_get_service_url(model_info, service_type, BASE_URL)
	}

	fn to_web_request_data(
		model_info: ModelInfo,
		config_set: &ConfigSet<'_>,
		service_type: ServiceType,
		chat_req: ChatRequest,
		chat_options: ChatOptionsSet<'_, '_>,
	) -> Result<WebRequestData> {
		// -- api_key (this Adapter requires it)
		let api_key = get_api_key_resolver(model_info.clone(), config_set)?;
		let url = Self::get_service_url(model_info.clone(), service_type);

		let web_req_data = OpenAIAdapter::util_to_web_request_data(model_info, url, chat_req, service_type, chat_options, &api_key);

		// Don't dump the header which contains the bearer tokens
		let _ = web_req_data.as_ref().inspect(|&wrd| {
			debug!("{:<12} - {}", "OpenAI.to_web_request_data", serde_json::to_string_pretty(&wrd.payload).unwrap());		
			}
		);

		web_req_data
	}

	fn to_chat_response(model_info: ModelInfo, web_response: WebResponse) -> Result<ChatResponse> {
		let WebResponse { mut body, .. } = web_response;

		debug!("{:<12} - OpenAI.to_chat_response {}", &model_info.model_name, serde_json::to_string_pretty(&body).unwrap());
		let usage = body.x_take("usage").map(OpenAIAdapter::into_usage).unwrap_or_default();

		// take one of the two branches based on finish_reason.
		let finish_reason:Option<String> = body.x_get("/choices/0/finish_reason")?;
		let first_choice: Option<Value> = body.x_take("/choices/0")?;		

		match finish_reason {
			Some(finish_reason) => {
				match finish_reason.as_str() {
					"stop" => {
						let res_content: Option<String> = first_choice
						.map(|mut c| c.x_take("/message/content"))
						.transpose()?;				
		
						Ok(
							ChatResponse { 
							payload: ChatResponsePayload::Content(res_content.map(MessageContent::from)), 
							usage }
						)
					},
					"tool_calls" => {
						let res_toolcall : Option<Value> = first_choice
						.map(|mut c| c.x_take("/message/tool_calls"))
						.transpose()?;				
		
						debug!("{:<12} -  {}", "OpenAI.to_web_request_data/tool_calls", 
							serde_json::to_string_pretty(&res_toolcall).unwrap());

						let tool_calls: Option<Vec<AssistantToolCall>> = serde_json::from_value(res_toolcall.unwrap())?;
		
						Ok(
							ChatResponse { 
							payload: ChatResponsePayload::ToolCall(tool_calls), 
							usage }
						)
					},
					_ => Err(Error::NeitherChatNorToolresponse { model_info }),
				}				
			},
			None => {
				Err(Error::UnexpectedChatResponseFormat{
					model_info, 
					detail: "OpenAI Adapter: /choices/0/finish_reason is missing".to_string()}
				)
			}
		}		
	}


	fn to_chat_stream(
		model_info: ModelInfo,
		reqwest_builder: RequestBuilder,
		options_sets: ChatOptionsSet<'_, '_>,
	) -> Result<ChatStreamResponse> {
		let event_source = EventSource::new(reqwest_builder)?;
		let openai_stream = OpenAIStreamer::new(event_source, model_info, options_sets);
		let chat_stream = ChatStream::from_inter_stream(openai_stream);

		Ok(ChatStreamResponse { stream: chat_stream })
	}
}

/// Support function for other Adapter that share OpenAI APIs
impl OpenAIAdapter {	

	pub(in crate::adapter::adapters) fn util_get_service_url(
		_model_info: ModelInfo,
		service_type: ServiceType,
		// -- util args
		base_url: &str,
	) -> String {
		match service_type {
			ServiceType::Chat | ServiceType::ChatStream => format!("{base_url}chat/completions"),
		}
	}

	pub(in crate::adapter::adapters) fn util_to_web_request_data(
		model_info: ModelInfo,
		url: String,
		chat_req: ChatRequest,
		service_type: ServiceType,
		options_set: ChatOptionsSet<'_, '_>,
		// -- utils args
		api_key: &str,
	) -> Result<WebRequestData> {
		let stream = matches!(service_type, ServiceType::ChatStream);

		// -- Build the header
		let headers = vec![
			// headers
			("Authorization".to_string(), format!("Bearer {api_key}")),
		];

		// -- Build the basic payload
		let model_name = model_info.model_name.to_string();
		let OpenAIRequestParts { messages, tools } = Self::into_openai_request_parts(model_info, chat_req)?;
		let mut payload = json!({
			"model": model_name,
			"messages": messages,
			"tools" : tools,
			"stream": stream
		});

		// -- Add options
		if let Some(true) = options_set.json_mode() {
			payload["response_format"] = json!({"type": "json_object"});
		}

		// --
		if stream & options_set.capture_usage().unwrap_or(false) {
			payload.x_insert("stream_options", json!({"include_usage": true}))?;
		}

		// -- Add supported ChatOptions
		if let Some(temperature) = options_set.temperature() {
			payload.x_insert("temperature", temperature)?;
		}
		if let Some(max_tokens) = options_set.max_tokens() {
			payload.x_insert("max_tokens", max_tokens)?;
		}
		if let Some(top_p) = options_set.top_p() {
			payload.x_insert("top_p", top_p)?;
		}

		Ok(WebRequestData { url, headers, payload })
	}

	/// Note: needs to be called from super::streamer as well
	pub(super) fn into_usage(mut usage_value: Value) -> MetaUsage {
		let input_tokens: Option<i32> = usage_value.x_take("prompt_tokens").ok();
		let output_tokens: Option<i32> = usage_value.x_take("completion_tokens").ok();
		let total_tokens: Option<i32> = usage_value.x_take("total_tokens").ok();
		MetaUsage {
			input_tokens,
			output_tokens,
			total_tokens,
		}
	}

	/// Takes the genai ChatMessages and build the OpenAIChatRequestParts
	/// - `genai::ChatRequest.system`, if present, goes as first message with role 'system'.
	/// - All messages get added with the corresponding roles (does not support tools for now)
	///
	/// NOTE: here, the last `true` is for the ollama variant
	///       It seems the Ollama compatibility layer does not work well with multiple System message.
	///       So, when `true`, it will concatenate the system message as a single on at the beginning
	fn into_openai_request_parts(model_info: ModelInfo, chat_req: ChatRequest) -> Result<OpenAIRequestParts> {
		use ChatMessage::*;

		let mut system_messages: Vec<String> = Vec::new();
		let mut messages: Vec<Value> = Vec::new();

		let ollama_variant = matches!(model_info.adapter_kind, AdapterKind::Ollama);	

		for msg in chat_req.messages {			
			match msg {
				System{content} => {
					// see note in the function comment
					if ollama_variant {
						system_messages.push(content);
					} else {
						messages.push(json!({"role": "system", "content": content}));
					}
				},
				Assistant {content, extra} => {
					let MessageContent::Text(content) = content;

					if let Some(MessageExtra::ToolCall(toolcall_v)) = extra {
						let mut tc_json_v = Vec::<Value>::new();
						for atc in toolcall_v {
							let func_args_string = match atc.function.fn_arguments {
								None => "".to_string(),
								Some(json_val) => json_val.to_string(),
							};

							tc_json_v.push(json!({
								"id" : atc.tool_call_id,
								"type" : atc.tool_call_type,
								"function": {
									"name": atc.function.fn_name,
									"arguments" : func_args_string
								}
							}));
						}

						messages.push(json! ({
							"role": "assistant", 
							"content": content,
							"tool_calls" : tc_json_v,							
						}));	
					}
					else {						
						messages.push(json! ({"role": "assistant", "content": content}));
					}
				},
    			User {content, ..} => {
					let MessageContent::Text(content) = content;
					messages.push(json! ({"role": "user", "content": content}));
				},
    			ToolResponse (tool_msg) => {
					messages.push(json!({
						"role" : "tool",
						"tool_call_id" : tool_msg.tool_call_id,
						"name" : tool_msg.tool_name,
						"content": tool_msg.tool_result,
					}));
				},
			}			
		}

		if !system_messages.is_empty() {
			let system_message = system_messages.join("\n");
			messages.insert(0, json!({"role": "system", "content": system_message}));
		}

		Ok(OpenAIRequestParts { messages, tools: chat_req.tools })
	}
}

// region:    --- Support

struct OpenAIRequestParts {
	messages: Vec<Value>,
	tools: Option<Vec<Value>>,
}

// endregion: --- Support
