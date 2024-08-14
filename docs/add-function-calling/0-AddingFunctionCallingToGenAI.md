# Adding function calling to genai

This is a missing feature in the `genai` library as of late July 2024. 

I had completed the gateway/worker split of the web-app and now have an API gateway that could use a `rust-genai` based worker to perform LLM based tasks for me. This is an ideal milestone to get started on the front-end workbench and explore how I could prompt engineer my way into previously NLP based solutions. However, recent light reading, brought _tool use_ to the top of my stack and I thought that I might as well grok it's use and use the full power of LLMs circa Fall 2024.

If my solution vocabulary was going to include LLM and Prompt Engineering, might as well complete it with tools and RAG just so I have all the jigsaw pieces. This lead me to start exploring adding it to JChnoe's `genai`. I think I managed to keep the code-quality and design considerations in the same ballpark as Jeremy. The changes are along these lines

 - Grow the `Tool` placeholder into something that works
 - Refactor ChatMessage and ChatRequest
 - Constraint tool-functions to a particular form and automate tool-schema creation
 - Add two examples related to function calling

It is always challenging to polish the product of an iterative coding process into a coherent design. Extra hard when you are working in the library of a super-competent developer with a highly evolved design sense. There are likely design choices I did not even consider: will see what feedback Chone has. The coding of this has been a great learning experience and any feedback will be too.

 - tool_call response
 - design changes
 - refactoring ChatMessage

# Investigation and design evaluations and example documentation

 - 👉 [c06-code-and-traces.md](./c06-code-and-traces.md) with activity diagrams of the full tool example.
 - 👉 [c07-code-and-traces.md](./c07-code-and-traces.md) with activity diagrams of the full tool example.
 - [1-ExploreAndProtoypeToolCalls.md](./1-ExploreAndProtoypeToolCalls.md)
 - [2-AutomatedPrompResponse.md](./2-AutomatedPromptResponse.md)
 - [3-PartiallyAutomateToolSchemasGeneration.md](./3-PartiallyAutomateToolSchemasGeneration.md)
 - [4-AutomateToolFunctionCalling.md](./4-AutomateToolFunctionCalling.md)


# Starting design state - Chat creation API

```rust
struct ChatRequest {
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
}

struct ChatMessage {
    pub role: ChatRole,
	pub content: MessageContent,
	pub extra: Option<MessageExtra>,
}

impl ChatMessage {
	pub fn system(content: impl Into<MessageContent>) -> Self {
		Self {
			role: ChatRole::System,
			content: content.into(),
			extra: None,
		}
	}

	pub fn assistant(content: impl Into<MessageContent>) -> Self {
		Self {
			role: ChatRole::Assistant,
			content: content.into(),
			extra: None,
		}
	}

	pub fn user(content: impl Into<MessageContent>) -> Self {
		Self {
			role: ChatRole::User,
			content: content.into(),
			extra: None,
		}
	}
}

#[derive(Debug, Clone)]
pub enum ChatRole {
	System,
	User,
	Assistant,
	Tool,
}

#[derive(Debug, Clone)]
pub enum MessageExtra {
	Tool(ToolExtra),
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct ToolExtra {
	tool_id: String,
}

pub enum MessageContent {
	Text(String),
}
```

# Final design state

I have added two examples to exercise this functionality. These are exhaustively described below 

 - 👉 [c06-code-and-traces.md](./c06-code-and-traces.md)
 - 👉 [c07-code-and-traces.md](./c07-code-and-traces.md)

```rust
#[derive(Debug, Clone)]
pub struct ToolMessage {
    pub tool_call_id: String,
	pub tool_name: String,

	// TODO: Keep this as structured Json and serialize at the end ?
	pub tool_result : String, 
}

#[derive(Debug, Clone)]
pub enum ChatMessage {    
    System       {content: String},
    Assistant    {content: MessageContent, extra: Option<MessageExtra>},
    User         {content: MessageContent, extra: Option<MessageExtra>},
    ToolResponse (ToolMessage)
}
```

 - Since `system` is just string, this is good. Can make it `MessageContent` as well but it will be inconsistent with the original `.with_system(String)`.
 - Potentially add a `MultiPartMessage` and base both `User` and `Assistant` on it. _This seems to be Chone's intent anyway, so might as well wait for his usual excellent job_.

## ChatRequest

### system field

ChatRequest contains multiple system messages that take different forms
   - `.system` which is a `Option<String>`
   - any number of `ChatMessage` items in `.messages` which contain a `MessageContent`

In retrospect, there was no need to streamline this. However, I decided to remove the `.system` field and simply assume that the client will put in a `ChatMessage.System` message in first.

Enforcing the first message to be a SystemMessage might be one way to go. However, `system` is an `Option<String>`: if it can be `None`, don't think there was an intent to ensure that the first message is a system message. If the default system message is fine, then we can simply get rid of the `.system` field.

```diff
pub struct ChatRequest {
-	pub system: Option<String>,
+	pub messages: Vec<ChatMessage>,
}
```

   - There are additional methods `ChatRequest.iter_systems()` and `ChatRequest.combine_systems` which concatenates all the system messages into one single system message. There is no use of these in the `genai` code base but I left them alone with slight modifications to account for the loss of the `ChatRequest.system` field.
     - Typically each chat has just one system messages
	 - Forums indicate that when using mutiple system messages, the idea is that each new system message applies to the subsequent user messages and acts like a break in the chat. 
	 - Not sure if merging system messages makes 

### tools

```diff
pub struct ChatRequest {	
	pub messages: Vec<ChatMessage>,
+	pub tools : Option<Vec<Value>>,
}

impl ChatRequest {
	...

+	pub fn append_tool(mut self, tool: Value) -> Self {		
+		self.tools.get_or_insert(Vec::new()).push(tool);
+		self
+	}
}
```

## ChatMessage and ChatRole

[ExploreAndProtoypeToolCalls.md](./1-ExploreAndProtoypeToolCalls.md) illustrated that the `role=Assistant` messages can be wildly different
 - The assistant response with multi-modal output (text, image, audio etc) are a good fit for the `MessageContent` payload (_whose future seems to be a Multi-Model data type_)
 - The assistant request for tool-calls is wildly different: it has function and function argument specifications. To me it looks like they force-fit a back-channel concept of tool-call into the messaging API. See [ExploreAndProtoypeToolCalls.md](./1-ExploreAndProtoypeToolCalls.md) to see how different a typical Assistant message is to the tool_call request.
 - The choice was to 
   - either creatively use `Option<MessageExtra>` with `role=Assistant|User` and handle both cases
   - Split the various types of responses with different enums _(having the same role)_

After some iterations (message creation all the way to adapter_impl), I decided to use both approaches for expediency.

### MessageExtra, Tool and AssitantTool

The `ToolExtra` class is a placeholder. During exploratory changes, I ended up creating an `AssistantToolCall` struct so simply decided to go with that instead.

```diff
#[derive(Debug, Clone)]
pub enum MessageExtra {
-	Tool(ToolExtra),
+	ToolCall(Vec<AssistantToolCall>),
}

- #[allow(unused)]
- #[derive(Debug, Clone)]
- pub struct ToolExtra {
- 	tool_id: String,
- }
```

The new `AssistantToolCall` and family looks like this:
 - Designed to de-serialize straight from OpenAI's JSON code. Not sure if this'll work for gemini, anthropic etc. Will see.
 - Function arguments are stringified json

```rust
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
```

### Split ChatRole from ChatMessage

> Iteratively testing out ideas and organically making edits can result in some loss of coherence. Most times, it is best to throw the iterative changes out and restart based on lessons learnt during the edits. In this case, the `ChatMessage` enums end up mapping `1:1` to `ChatRole`. Maybe having the same `MessageContent` and `MessageExtra` cover the varying data needs would have been ok (_not great, but ok_) and resulted in minimal code changes and review burden. If the user Experience is mostly via `ChatMessage.system|assistant|user|tool()` factory methods, the `ChatMessage` ergonomics and clarity might not be a major concern. Will wait for Chone's comments and defer to his preference.

I decided to use the following enum classes to hold messages. With a new `ToolMessage` instead of updating `MessageExtra`.

```rust
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
```

```diff
#[derive(Debug, Clone)]
pub struct ChatMessage {
-	pub role: ChatRole,
-	pub content: MessageContent,
-	pub extra: Option<MessageExtra>,
+	System       {content: String},
+   Assistant    {content: MessageContent, extra: Option<MessageExtra>},
+   User         {content: MessageContent, extra: Option<MessageExtra>},
+   ToolResponse (ToolMessage)
}
```

Generate `ChatRole` from the `ChatMessage` types via

```rust
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
```

One of the tradeoff's with this decision is that
 - ❌ If we overload the `Option<MessageExtra>` to carry the various forms, then `adapter_impl` will end up doing a nested `match` (On top of a `match` on `ChatRole`). The developer ergonomics might suffer because widely different usages are stuffed into `MessageExtra`
 - ✔️ If we use different types (_enums_) based on semantics, then developer ergonomics will be clearer but `adapter_impl` changes will need to exhaustively match on enums: worse for lines of code but easier for compiler to enforce exhaustivity. 
 - Otoh, if the message creation semantics are hidden behind an expressive `ChatMessage.system|assistant|user|tool()`, will any of this even matter ?
 

Changes to the factory followed
 - assistant creation now uses the `MessageExtra` to hold the tool_call info. Could have equally decided to go with a new `ChatMessage::ToolCallRequest` or similar.
 - new `impl Into<MessageExtra>` following the existing pattern of `impl Into<MessageContent>`
 - _For all of rust's great features. I miss C++'s overloading and default parameter values_

```diff
/// Constructors
impl ChatMessage {	
	pub fn system(content: impl Into<String>) -> Self {
		Self::System {
			content: content.into(),
		}
	}

	pub fn assistant(content: impl Into<MessageContent>) -> Self {
-		Self {
-			role: ChatRole::Assistant,		
		Self::Assistant{
			content: content.into(),
			extra: None
		}
	}

+	pub fn assistant_with_extra(content: impl Into<MessageContent>, extra: impl Into<MessageExtra>) -> Self {
+		Self::Assistant{
+			content: content.into(),
+			extra: Some(extra.into())
		}
	}

	pub fn user(content: impl Into<MessageContent>) -> Self {
-		Self {
-			role: ChatRole::User,	
+		Self::User {
			content: content.into(),
			extra: None
		}
	}

+	pub fn tool_response(tool_call_id: String, tool_name: String, tool_result: String) -> Self {
+		Self::ToolResponse(ToolMessage{tool_call_id, tool_name, tool_result})
+	}
}
```

Also added converters from the `AssistantToolCall` structs 

```rust
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
```

## ChatResponse

Since the `ChatResponse` now accounts for tool_calls, I made a similar choice of representing `MessageContent` and `AssistantToolCall` as a sum type. A new `payload` field holds the sum type.

```diff
#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
+	pub payload: ChatResponsePayload,
-	pub content: Option<MessageContent>,
	pub usage: MetaUsage,
}
```

With the `ChatResponsePayload` looking like this

```rust
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
```

👉 The getters have been modified as needed but needs a closer look to match the original intent of these getters.

## Tool schema and invokers

These are larger topics and covered by 

 - [3-PartiallyAutomateToolSchemasGeneration.md](./3-PartiallyAutomateToolSchemasGeneration.md)
 - [4-AutomateToolFunctionCalling.md](./4-AutomateToolFunctionCalling.md)
 - [5-HardenToolCalling.md](./5-HardenToolCalling.md)


## New errors

```diff
+	UnexpectedChatResponseFormat {
+		model_info: ModelInfo,
+		detail : String,
+	},
+
+	NeitherChatNorToolresponse {
+		model_info: ModelInfo
+	},

....

+  #[from]
+	SerdeJson(serde_json::Error),
```

## adapter_impl changes - Anthropic and Gemini

The adapter changes follow a similar pattern: _matching over `message.role` changed to matching over `ChatMessage` variants_. 

From this 

```rust
for msg in chat_req.messages {
	// Note: Will handle more types later
	let MessageContent::Text(content) = msg.content;

	match msg.role {
		// for now, system and tool goes to system
		ChatRole::System | ChatRole::Tool => systems.push(content),
		ChatRole::User => messages.push(json! ({"role": "user", "content": content})),
		ChatRole::Assistant => messages.push(json! ({"role": "assistant", "content": content})),
	}
}
```

to

```rust
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
```

## adapter_impl changes - Cohere

There are some non-trivial changes here to account for a check that the last-message is a User message but otherwise same as Anthropic and Gemini

## adapter_impl changes - OpenAI and Groq

OpenAI has some changes to handle tool calling. To make life easier, I also added tracing
  - `to_web_request_data` debug dumps the `web_request_data` object. Explicitly avoids the header so the API_KEY is not written in the trace.
  - `to_chat_response` also debug dumps the incoming json body.

See [1-ExploreAndProtoypeToolCalls.md](./1-ExploreAndProtoypeToolCalls.md) for the JSON that OpenAI returns and considerations for the code changes. I am just listing the summary below.

With tool_calling, the logic has changed because
 - `message/content` is null when tool_calls are supplied. This null was handled.
 - `message/tool_calls` carries the new tool_call payload.
 - `choices/0/finish_reason` is used to figure out whether we shuold read the assistant response `(finish_reason=stop)` content or tool_calls `(finish_reason=tool_calls)`
 - if tool_calls, the `messages/tool_calls` is deserialized into `Option<Vec<AssistantToolCall>>`
 - Since we are now depending on the `finish_reason`, Logic has been updated to throw an error if it is neither `stop` nor `tool_calls`.


`util_to_web_request_data` is updated to include the _tools_ in the API packet.

```diff
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
-		let OpenAIRequestParts { messages} = Self::into_openai_request_parts(model_info, 
+		let OpenAIRequestParts { messages, tools } = Self::into_openai_request_parts(model_info, chat_req)?;
		let mut payload = json!({
			"model": model_name,
			"messages": messages,
+			"tools" : tools,
			"stream": stream
		});
		....
```

`into_openai_request_parts` is updated to handle parsing the new tool_call requests. Primarily

  - Note that `Assistant` destructures the `extra` field as well and converts it all to json
  - Newly handles the tool response

```rust
for msg in chat_req.messages {			
	match msg {

		....snip...

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

		....snip....

		ToolResponse (tool_msg) => {
			messages.push(json!({
				"role" : "tool",
				"tool_call_id" : tool_msg.tool_call_id,
				"name" : tool_msg.tool_name,
				"content": tool_msg.tool_result,
			}));
		},
```

`OpenIARequestParts` is updated to hold a tools array

```diff
struct OpenAIRequestParts {
	messages: Vec<Value>,
+	tools: Option<Vec<Value>>,
}
```