use serde::{de, Deserialize};
use serde_json::Value;

pub enum ToolChoice {
	None,
	Auto,

	// translate into
	// [{"type": "function", "function": {"name": "my_function"}}].
	RequiredFunctions(Vec<String>),
}

//------------------------------------------------------------
// OpenAI/Groq tool-call response
//  - "arguments" are stringified json
//-------------------------------------------------------------
// "tool_calls": [
// 	{
// 	  "function": {
// 		"arguments": "{\"format\":\"fahrenheit\",\"location\":\"San Jose, CA\"}",
// 		"name": "get_current_weather"
// 	  },
// 	  "id": "call_Vu0c1G8RZMFxebzkQfa7V8VJ",
// 	  "type": "function"
// 	}
//-------------------------------------------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantToolCall {
	#[serde(rename="id")]
	pub tool_call_id: String,
	
	#[serde(rename="type")]	
	pub tool_call_type: String,

	pub function: AssistantToolCallFunction,	
}

#[derive(Deserialize, Debug, Clone)]
pub struct AssistantToolCallFunction {
	#[serde(rename="name")]
	pub fn_name : String,

	#[serde(rename="arguments")]
	#[serde(deserialize_with = "deserialize_json_string")]
	pub fn_arguments : Option<Value>,
}

fn deserialize_json_string<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s: String = de::Deserialize::deserialize(deserializer)?;
    serde_json::from_str(&s)
        .map(|v| Some(v))
        .map_err(de::Error::custom)
}

