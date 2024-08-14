//! This module contains all the types related to a Chat Request (except ChatOptions, which has its own file).

use serde_json::Value;
use crate::chat::MessageContent;
use crate::chat::tool::AssistantToolCall;

// region:    --- ChatRequest

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {	
	pub messages: Vec<ChatMessage>,
	pub tools : Option<Vec<Value>>,
}

/// Constructors
impl ChatRequest {
	pub fn new(messages: Vec<ChatMessage>) -> Self {
		Self { messages, tools: None}
	}

	/// From `.system` property content.
	pub fn from_system(content: impl Into<String>) -> Self {
		let obj = Self::new(Vec::new());
		obj.with_system(content)		 
	}
}

/// Chainable Setters
impl ChatRequest {
	pub fn with_system(mut self, system: impl Into<String>) -> Self {
		self.messages.push(
			ChatMessage::System{
				content: system.into()}
		);

		self
	}	

	/// Use the various ChatMessage::XX items to create the ChatMessage
	/// instance to append
	pub fn append_message(mut self, msg: ChatMessage) -> Self {
		self.messages.push(msg);
		self
	}		

	pub fn append_tool(mut self, tool: Value) -> Self {		
		self.tools.get_or_insert(Vec::new()).push(tool);
		self
	}
}

/// Getters
impl ChatRequest {
	/// Iterate through all of the system content
	pub fn iter_systems(&self) -> impl Iterator<Item = &str> {		
		self
			.messages			
			.iter()
			.filter_map(|message| match message {
				ChatMessage::System { content } => Some(content.as_str()),
				_ => None,
			})
	}
	
	/// Combine the eventual ChatRequest `.system` and system messages into one string.
	/// - It will start with the evnetual `chat_request.system`
	/// - Then concatenate the eventual `ChatRequestMessage` of Role `System`
	/// - This will attempt to add an empty line between system content. So, it will add
	///   - Two `\n` when the prev content does not end with `\n`
	///   - and one `\n` if the prev content ends with `\n`
	pub fn combine_systems(&self) -> Option<String> {
		let mut systems: Option<String> = None;

		for system in self.iter_systems() {
			let systems_content = systems.get_or_insert_with(|| "".to_string());

			// add eventual separator
			if systems_content.ends_with('\n') {
				systems_content.push('\n');
			} else if !systems_content.is_empty() {
				systems_content.push_str("\n\n");
			} // do not add any empyt line if prev content is empty

			systems_content.push_str(system);
		}

		systems
	}
}

// endregion: --- ChatRequest

// region:    --- ChatMessage

#[derive(Debug, Clone)]
pub struct ToolMessage {
    pub tool_call_id: String,
	pub tool_name: String,	
	pub tool_result : String,
}

#[derive(Debug, Clone)]
pub enum ChatMessage {    
	System       {content: String},
    Assistant    {content: MessageContent, extra: Option<MessageExtra>},
    User         {content: MessageContent, extra: Option<MessageExtra>},
    ToolResponse (ToolMessage)
}

/// Constructors
impl ChatMessage {	
	pub fn system(content: impl Into<String>) -> Self {
		Self::System {
			content: content.into(),
		}
	}

	pub fn assistant(content: impl Into<MessageContent>) -> Self {
		Self::Assistant{
			content: content.into(),
			extra: None
		}
	}

	pub fn assistant_with_extra(content: impl Into<MessageContent>, extra: impl Into<MessageExtra>) -> Self {
		Self::Assistant{
			content: content.into(),
			extra: Some(extra.into())
		}
	}

	pub fn user(content: impl Into<MessageContent>) -> Self {
		Self::User {
			content: content.into(),
			extra: None
		}
	}

	pub fn tool_response(tool_call_id: String, tool_name: String, tool_result: String) -> Self {
		Self::ToolResponse(ToolMessage{tool_call_id, tool_name, tool_result})
	}
}

// Implementation to convert AssistantToolCalls into MessageExtras
impl From<Vec<AssistantToolCall>> for MessageExtra 
{
	fn from(atc_v: Vec<AssistantToolCall>) -> Self {
		MessageExtra::ToolCall(atc_v)
	}
}	

// Implementation to convert AssistantToolCalls into ChatMessage
impl From<Vec<AssistantToolCall>> for ChatMessage
{
	fn from(atc_v: Vec<AssistantToolCall>) -> Self {
		ChatMessage::Assistant{
			content:"".into(), 
			extra: Some(MessageExtra::ToolCall(atc_v))}
	}
}	

#[derive(Debug, Clone)]
pub enum ChatRole {
	System,
	User,
	Assistant,
	Tool,
}

impl From<ChatMessage> for ChatRole {
	fn from(msg:ChatMessage) -> Self {
		match msg {
			ChatMessage::System {..} => Self::System,
			ChatMessage::Assistant {..} => Self::Assistant,
			ChatMessage::User{..} => Self::User,
			ChatMessage::ToolResponse(_) => Self::Tool,
		}
	}
}

#[derive(Debug, Clone)]
pub enum MessageExtra {
	ToolCall(Vec<AssistantToolCall>),
}

// TODO: Remove this
// #[allow(unused)]
// #[derive(Debug, Clone)]
// pub struct ToolExtra {
// 	tool_id: String,
// }

// endregion: --- ChatMessage
