//! This module contains all the types related to a Chat Response (except ChatStream which has it file).

use crate::chat::{ChatStream, MessageContent};
use crate::chat::tool::AssistantToolCall;

// region:    --- ChatResponse
#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
	pub payload: ChatResponsePayload,
	pub usage: MetaUsage,
}

#[derive(Debug, Clone)]
pub enum ChatResponsePayload {
	Content(Option<MessageContent>),
	ToolCall(Option<Vec<AssistantToolCall>>),
}

impl Default for ChatResponsePayload {
	fn default() -> Self {
		Self::Content(None)
	}
}

// Getters
impl ChatResponse {	

	/// Returns the eventual content as Option<&MessageContent>
	/// Can be none if the response payload is a tool_call
	pub fn content_as_ref(&self) -> Option<&MessageContent> {
		match &self.payload {
			ChatResponsePayload::Content(opt_mc) => opt_mc.as_ref(),
			ChatResponsePayload::ToolCall(_) => None,
		}		
	}

	/// Returns the eventual content as `&str` if it is of type `MessageContent::Text`
	/// Otherwise, return None
	pub fn content_text_as_str(&self) -> Option<&str> {
		match &self.payload {
			ChatResponsePayload::Content(opt_mc) => opt_mc.as_ref().and_then(MessageContent::text_as_str),
			ChatResponsePayload::ToolCall(_) => None,
		}		
	}

	/// Consume the ChatResponse and returns the eventual String content of the `MessageContent::Text`
	/// Otherwise, return None
	pub fn content_text_into_string(self) -> Option<String> {
		match self.payload {
			ChatResponsePayload::Content(opt_mc) => opt_mc.and_then(MessageContent::text_into_string),
			ChatResponsePayload::ToolCall(_) => None,
		}				
	}
}

// endregion: --- ChatResponse

// region:    --- ChatStreamResponse

pub struct ChatStreamResponse {
	pub stream: ChatStream,
}

// endregion: --- ChatStreamResponse

// region:    --- MetaUsage

/// IMPORTANT: This is **NOT SUPPORTED** for now. To show the API direction.
#[derive(Default, Debug, Clone)]
pub struct MetaUsage {
	pub input_tokens: Option<i32>,
	pub output_tokens: Option<i32>,
	pub total_tokens: Option<i32>,
}

// endregion: --- MetaUsage
