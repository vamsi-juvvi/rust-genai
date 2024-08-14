use crate::adapter::anthropic::AnthropicStreamer;
use crate::adapter::support::get_api_key_resolver;
use crate::adapter::{Adapter, AdapterConfig, AdapterKind, ServiceType, WebRequestData};
use crate::chat::{
	ChatMessage, ChatOptionsSet, ChatRequest, ChatResponse, ChatResponsePayload, ChatRole, ChatStream, ChatStreamResponse, MessageContent, MetaUsage
};
use crate::support::value_ext::ValueExt;
use crate::webc::WebResponse;
use crate::{Error, Result};
use crate::{ConfigSet, ModelInfo};
use reqwest::RequestBuilder;
use reqwest_eventsource::EventSource;
use serde_json::{json, Value};
use std::sync::OnceLock;

pub struct AnthropicAdapter;

const BASE_URL: &str = "https://api.anthropic.com/v1/";
const MAX_TOKENS: u32 = 1024;
const ANTRHOPIC_VERSION: &str = "2023-06-01";
const MODELS: &[&str] = &[
	"claude-3-5-sonnet-20240620",
	"claude-3-opus-20240229",
	"claude-3-sonnet-20240229",
	"claude-3-haiku-20240307",
];

impl Adapter for AnthropicAdapter {
	/// Note: For now returns the common ones (see above)
	async fn all_model_names(_kind: AdapterKind) -> Result<Vec<String>> {
		Ok(MODELS.iter().map(|s| s.to_string()).collect())
	}

	fn default_adapter_config(_kind: AdapterKind) -> &'static AdapterConfig {
		static INSTANCE: OnceLock<AdapterConfig> = OnceLock::new();
		INSTANCE.get_or_init(|| AdapterConfig::default().with_auth_env_name("ANTHROPIC_API_KEY"))
	}

	fn get_service_url(_model_info: ModelInfo, service_type: ServiceType) -> String {
		match service_type {
			ServiceType::Chat | ServiceType::ChatStream => format!("{BASE_URL}messages"),
		}
	}

	fn to_web_request_data(
		model_info: ModelInfo,
		config_set: &ConfigSet<'_>,
		service_type: ServiceType,
		chat_req: ChatRequest,
		options_set: ChatOptionsSet<'_, '_>,
	) -> Result<WebRequestData> {
		let model_name = model_info.model_name.clone();

		let stream = matches!(service_type, ServiceType::ChatStream);
		let url = Self::get_service_url(model_info.clone(), service_type);

		// -- api_key (this Adapter requires it)
		let api_key = get_api_key_resolver(model_info.clone(), config_set)?;

		let headers = vec![
			// headers
			("x-api-key".to_string(), api_key.to_string()),
			("anthropic-version".to_string(), ANTRHOPIC_VERSION.to_string()),
		];

		let AnthropicRequestParts { system, messages } = Self::into_anthropic_request_parts(model_info, chat_req)?;

		// -- Build the basic payload
		let mut payload = json!({
			"model": model_name.to_string(),
			"messages": messages,
			"stream": stream
		});
		if let Some(system) = system {
			payload.x_insert("system", system)?;
		}

		// -- Add supported ChatOptions
		if let Some(temperature) = options_set.temperature() {
			payload.x_insert("temperature", temperature)?;
		}

		let max_tokens = options_set.max_tokens().unwrap_or(MAX_TOKENS);
		payload.x_insert("max_tokens", max_tokens)?; // required for anyhropic

		if let Some(top_p) = options_set.top_p() {
			payload.x_insert("top_p", top_p)?;
		}

		Ok(WebRequestData { url, headers, payload })
	}

	fn to_chat_response(_model_info: ModelInfo, web_response: WebResponse) -> Result<ChatResponse> {
		let WebResponse { mut body, .. } = web_response;
		let json_content_items: Vec<Value> = body.x_take("content")?;

		let mut content: Vec<String> = Vec::new();

		let usage = body.x_take("usage").map(Self::into_usage).unwrap_or_default();

		for mut item in json_content_items {
			let item_text: String = item.x_take("text")?;
			content.push(item_text);
		}

		let content = if content.is_empty() {
			None
		} else {
			Some(content.join(""))
		};
		let content = content.map(MessageContent::from);

		Ok(ChatResponse { 
			payload: ChatResponsePayload::Content(content), 
			usage }
		)
	}

	fn to_chat_stream(
		model_info: ModelInfo,
		reqwest_builder: RequestBuilder,
		options_set: ChatOptionsSet<'_, '_>,
	) -> Result<ChatStreamResponse> {
		let event_source = EventSource::new(reqwest_builder)?;
		let anthropic_stream = AnthropicStreamer::new(event_source, model_info, options_set);
		let chat_stream = ChatStream::from_inter_stream(anthropic_stream);
		Ok(ChatStreamResponse { stream: chat_stream })
	}
}

// region:    --- Support

impl AnthropicAdapter {
	pub(super) fn into_usage(mut usage_value: Value) -> MetaUsage {
		let input_tokens: Option<i32> = usage_value.x_take("input_tokens").ok();
		let output_tokens: Option<i32> = usage_value.x_take("output_tokens").ok();

		// Compute total_tokens
		let total_tokens = if input_tokens.is_some() || output_tokens.is_some() {
			Some(input_tokens.unwrap_or(0) + output_tokens.unwrap_or(0))
		} else {
			None
		};

		MetaUsage {
			input_tokens,
			output_tokens,
			total_tokens,
		}
	}

	/// Takes the genai ChatMessages and build the System string and json Messages for Anthropic.
	/// - Will push the `ChatRequest.system` and systems message to `AnthropicsRequestParts.system`
	fn into_anthropic_request_parts(model_info: ModelInfo, chat_req: ChatRequest) -> Result<AnthropicRequestParts> {
		use ChatMessage::*;

		let mut messages: Vec<Value> = Vec::new();
		let mut systems: Vec<String> = Vec::new();		

		for msg in chat_req.messages {						

			match msg {
				System{content} =>  systems.push(content),				
				Assistant {content, ..} => 
				{
					let MessageContent::Text(content) = content;
					messages.push(json! ({"role": "assistant", "content": content}))
				},
    			User {content, ..} => 
				{
					let MessageContent::Text(content) = content;
					messages.push(json! ({"role": "user", "content": content}))
				},
    			ToolResponse (_tool_msg) => 
				{
					return Err(Error::MessageRoleNotSupported {
						model_info,
						role: ChatRole::Tool,
					});
				},
			}			
		}

		let system = if !systems.is_empty() {
			Some(systems.join("\n"))
		} else {
			None
		};

		Ok(AnthropicRequestParts { system, messages })
	}
}

struct AnthropicRequestParts {
	system: Option<String>,
	messages: Vec<Value>,
	// TODO: need to add tools
}

// endregion: --- Support
