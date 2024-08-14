# Exploring tool-calls

I started with https://cookbook.openai.com/examples/how_to_call_functions_with_chat_models as the example. Picked their examples as-is and decided to send plain json in first so I could study the API response.

```json
[
    {
        "type": "function",
        "function": {
            "name": "get_current_weather",
            "description": "Get the current weather",
            "parameters": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA",
                    },
                    "format": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"],
                        "description": "The temperature unit to use. Infer this from the users location.",
                    },
                },
                "required": ["location", "format"],
            },
        }
    },
    {
        "type": "function",
        "function": {
            "name": "get_n_day_weather_forecast",
            "description": "Get an N-day weather forecast",
            "parameters": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA",
                    },
                    "format": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"],
                        "description": "The temperature unit to use. Infer this from the users location.",
                    },
                    "num_days": {
                        "type": "integer",
                        "description": "The number of days to forecast",
                    }
                },
                "required": ["location", "format", "num_days"]
            },
        }
    },
]
```

### Initial example to drive the tool call

```rust
async fn main() -> Result<(), Box<dyn std::error::Error>> {


	let client = Client::default();

	let mut chat_req = ChatRequest::default().with_system(
        "Don't make assumptions about what values to plug into functions. Ask for clarification if a user request is ambiguous."
    );

    // Specify the name of the city to make sure it calls the `get_current_weather` tool.
    chat_req = chat_req.append_message(ChatMessage::user("What's the weather like today in San Jose, CA")); 

    // Append the function tools JSON per
    // https://cookbook.openai.com/examples/how_to_call_functions_with_chat_models 
    chat_req = chat_req.append_tool(
        json!({
                "type": "function",
                "function": {
                    "name": "get_current_weather",
                    "description": "Get the current weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {
                                "type": "string",
                                "description": "The city and state, e.g. San Francisco, CA",
                            },
                            "format": {
                                "type": "string",
                                "enum": ["celsius", "fahrenheit"],
                                "description": "The temperature unit to use. Infer this from the users location.",
                            },
                        },
                        "required": ["location", "format"],
                    },
                }
            })
    );

    chat_req = chat_req.append_tool(
        json!({
            "type": "function",
            "function": {
                "name": "get_n_day_weather_forecast",
                "description": "Get an N-day weather forecast",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city and state, e.g. San Francisco, CA",
                        },
                        "format": {
                            "type": "string",
                            "enum": ["celsius", "fahrenheit"],
                            "description": "The temperature unit to use. Infer this from the users location.",
                        },
                        "num_days": {
                            "type": "integer",
                            "description": "The number of days to forecast",
                        }
                    },
                    "required": ["location", "format", "num_days"]
                },
            }
        }));
    
    let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;
	println!("{}", chat_res.content_text_as_str().unwrap_or("NO ANSWER"));    

	Ok(())
}
```

### Send the tools all the way to the endpoint

**ChatRequest**

```diff

#[derive(Debug, Clone, Default)]
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

**openai/adapter_impl.rs**

```diff
-    let OpenAIRequestParts { messages } = Self::into_openai_request_parts(model_info, chat_req)?;
+	let OpenAIRequestParts { messages, tools } = Self::into_openai_request_parts(model_info, chat_req)?;
		let mut payload = json!({
			"model": model_name,
			"messages": messages,
+			"tools" : tools,
			"stream": stream
		});

	fn into_openai_request_parts(model_info: ModelInfo, chat_req: ChatRequest) -> Result<OpenAIRequestParts> {
        ...
-		Ok(OpenAIRequestParts { messages})
+       Ok(OpenAIRequestParts { messages, tools: chat_req.tools })
	}

struct OpenAIRequestParts {
	messages: Vec<Value>,
+	tools: Option<Vec<Value>>,
}        
```

## adjust input

This worked! Mostly! `gpt-4o-mini` is supposed to be hot stuff. However, it totally ignored the `"description": "The temperature unit to use. Infer this from the users location.",` part of the tool's `format` parameter.

I got back a response that asked: _Would you like the temperature in Celcius or Fahrenheit?_. Yay that It did read the tools properly _(no surprise though since this was copied from their cookbook_).

Let me update the instructions to give me weather in Fahrtenheit and get to the actual tool_calls.

```diff
chat_req = chat_req
    .append_message(
-        ChatMessage::user("What's the weather like today in San Jose, CA")
+        ChatMessage::user("What's the weather like today in San Jose, CA. Provide the temperature in fahrenheits.")
        );
```

## fix the output parse to expect null for content output

Now I got an error. Likely need to update output processing to handle tool calls. Added `println!("{}", serde_json::to_string_pretty(&body).unwrap());` to the `OpenAI Adapter`'s `to_chat_response(_model_info: ModelInfo, web_response: WebResponse)` method and got 

```json
{
  "choices": [
    {
      "finish_reason": "tool_calls",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": null,
        "role": "assistant",
        "tool_calls": [
          {
            "function": {
              "arguments": "{\"format\":\"fahrenheit\",\"location\":\"San Jose, CA\"}",
              "name": "get_current_weather"
            },
            "id": "call_VJFPBE7DkRAynPGKvbIOhnI4",
            "type": "function"
          }
        ]
      }
    }
  ],
  "created": 1722224480,
  "id": "chatcmpl-9qBY8tnZulLZbQbz4jKzTXf0qtYO8",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_ba606877f9",
  "usage": {
    "completion_tokens": 23,
    "prompt_tokens": 195,
    "total_tokens": 218
  }
}
```

 - `message/content` is null!
 - `tool_calls` is an array
 - Each `tool_call` is a function invocation with an `id`
 - The `id` ties the tool_call request with the subsequent `role=user` tool_call response message


When the above json gets processed, I get this error:

```
Error: XValue(SerdeJson(Error("invalid type: null, expected a string", line: 0, column: 0)))
```


Which is likely cause by the read of the `/message/content` xpath since it is null.

```rust
let first_choice: Option<Value> = body.x_take("/choices/0")?;
let content: Option<String> = first_choice.map(|mut c| c.x_take("/message/content")).transpose()?;
```        

The `x_take` method does a `serde_json::from_value(value)?;` and throws the above error when it finds a null!

### tool_call parse first cut

 - Get both "message/content" and "message/tool_calls" as options
 - Error out if neither or both
 - Process each separately

```rust
fn to_chat_response(model_info: ModelInfo, web_response: WebResponse) -> Result<ChatResponse> {
    let WebResponse { mut body, .. } = web_response;

    println!("{}", serde_json::to_string_pretty(&body).unwrap());

    let usage = body.x_take("usage").map(OpenAIAdapter::into_usage).unwrap_or_default();
    
    let first_choice: Option<Value> = body.x_take("/choices/0")?;

    let content_res: Result<Option<String>> = first_choice.clone()
        .map(|mut c| c.x_take("/message/content"))
        .transpose()
        .map_err(|e| e.into());

    let toolcall_res : Result<Option<Value>> = first_choice
        .map(|mut c| c.x_take("/message/tool_calls"))
        .transpose()
        .map_err(|e| e.into());

    match (content_res, toolcall_res) {
        (Ok(c), Ok(t)) => Err(Error::BothChatAndToolresponse { model_info }),			
        (Ok(c), _) => {				
            Ok(
                ChatResponse { 
                payload: ChatResponsePayload::Content(c.map(MessageContent::from)), 
                usage }
            )
        },
        (_, Ok(t)) => {
            let tool_calls: Option<Vec<AssistantToolCall>> = serde_json
                ::from_value(t.unwrap())?;

            Ok(
                ChatResponse { 
                payload: ChatResponsePayload::ToolCall(tool_calls), 
                usage }
            )
        },
        (Err(_), Err(_)) => Err(Error::NeitherChatNorToolresponse { model_info }),
    }		
}
```

On a second read, this looks perverse. Since I have the `/choices/0/finish_reason`, I shuold know to expect either `message/content` or `message/tool_calls`.

### tool_call parse based on finish_reason

The `finish_reason` json flag is a good way to figure out what the response contains. I don't have to check to see which of the `content | tool_calls` is not-null.

```rust
fn to_chat_response(model_info: ModelInfo, web_response: WebResponse) -> Result<ChatResponse> {
    let WebResponse { mut body, .. } = web_response;
    
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
```
### AssistantToolCall

I refactored `tool` into it's own module `genai.chat.tool` and finalized the following for `AssistantToolCall`. The name is preliminary, _Assistant Calls for tool invocation_ hence `AssistantToolCall`, could have been `ToolCall` too. The things to note is that the `Option<Value>` is serialized to a stringified json to match what the chat-endpoint sends us. The rest is simply mapping the descriptive field-names into the generic names the json serialization wants.

```rust
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

I am including the tool_call json as a reference. The various `rename=".."` come from reverse-engineering the json below.

```json
{
  "choices": [
    {
      "finish_reason": "tool_calls",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": null,
        "role": "assistant",
        "tool_calls": [
          {
            "function": {
              "arguments": "{\"format\":\"fahrenheit\",\"location\":\"San Jose, CA\"}",
              "name": "get_current_weather"
            },
            "id": "call_VJFPBE7DkRAynPGKvbIOhnI4",
            "type": "function"
          }
        ]
      }
    }
  ],
  "created": 1722224480,
  "id": "chatcmpl-9qBY8tnZulLZbQbz4jKzTXf0qtYO8",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_ba606877f9",
  "usage": {
    "completion_tokens": 23,
    "prompt_tokens": 195,
    "total_tokens": 218
  }
}
```

## Send a tool_call response message

I already put some code in for this while refactoring `ChatMessage`. Continuing with that

```diff
impl ChatMessage {
    ..
	pub fn user(content: impl Into<MessageContent>) -> Self {

+	pub fn tool(tool_call_id: String, tool_name: String, tool_result: String) -> Self {
+		Self::Tool(ToolMessage{tool_call_id, tool_name, tool_result})
+	}
}
```

`ToolMessage` is defined with field-names that are descriptive and helpful to the developer. They'll be serialized to match the API expectations. I am not doing it via serialization _(since different adapters might want it differently. However, am hoping they all follow the OpenAI example though)_.

**This example code** shows the field-names expected in the message.

```python
 messages.append({
            "role":"tool", 
            "tool_call_id":tool_call_id, 
            "name": tool_function_name, 
            "content":results
        })
```

Will be contained in this struct

```rust
#[derive(Debug, Clone)]
pub struct ToolMessage {
    pub tool_call_id: String,
	pub tool_name: String,	
	pub tool_result : String,
}
```

**Modify the adapter** to process tool messages

```diff
    Tool (tool_msg) => {
-            return Err(Error::MessageRoleNotSupported {
-                model_info,
-                role: ChatRole::Tool,
-            });
+            messages.push(json!({
+				"role" : "tool",
+				"tool_call_id" : tool_msg.tool_call_id,
+				"name" : tool_msg.tool_name,
+				"content": tool_msg.tool_result,
+			}));
        },
```

**Modified the test** to continue the message loop

```rust
 loop {
        let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;

        let mut followup_msg:Option<ChatMessage> = None;

        match chat_res.payload {
            ChatResponsePayload::Content(opt_mc) => {
                println!("-------------------");
                println!("Got Response\n\n");
                println!("{}", opt_mc
                    .as_ref()
                    .and_then(MessageContent::text_as_str)
                    .unwrap_or("NO ANSWER"));
            },
            ChatResponsePayload::ToolCall(opt_tc) => {            
                if let Some(tc_vec) =  opt_tc {
                    println!("-------------------");
                    println!("Responding to tool_calls\n\n {:?}", &tc_vec);

                    for tool_call in tc_vec {
                        // Fake it for now. Simply return 75F
                        let tool_response_msg = ChatMessage::tool(
                            tool_call.tool_call_id, 
                            tool_call.function.fn_name,
                            "75F".to_string());
                        
                            followup_msg.replace(tool_response_msg);

                            println!("-------------------");
                            println!("Adding tool_call response msg\n\n {:?}", &followup_msg);
                    }
                }
            }
        }    

        // Continue chat as long as we have followup messages
        if let Some(msg) = followup_msg {
            chat_req = chat_req.append_message(msg);
        } else {
            break;
        }
    }	
```

## OpenAI had additional requirements

Turns out, this is enough for `groq`. However, `OpenAI` wants the previous `tool_calls` response also added to the chat history. Any `role=tool` message needs to be immediately be preceded by a `tool_calls` message apparently. Looks like this is standard, any response from the assistant has to be added to the context for it to be coherent.

So. Currently, the `role=assistant` just takes a text input. Now it also needs to take a `tool_calls` input. 

I have two options here,
 - Add it to the multi-part mime `MessageContent`
 - ❌Add a whole new `AssistantToolCall(...)` enum for `ChatMessages`
   - `Assistant` → `AssistantPrompt`
   - ➕ `AssistantToolCall`
 - There is already a `MessageExtra` enum with `Tool(ToolExtra)` and signatures for it.
   - This field is unused in Adapters as the pattern matche put it under `..` and ignores it. A risk when you have fields you are ignoring vs an entire enum-variant that you are ignoring (_which is more visible in the code_).
   - But can change to `Tool(Vec<AssistantToolCall>)` and re-use it.


**Add new ctor**

```diff
impl ChatMessage {	
	...

+	pub fn assistant_with_extra(content: impl Into<MessageContent>, extra: Option<MessageExtra>) -> Self {
+		Self::Assistant{
+			content: content.into(),
+			extra: extra
+		}
+	}

	....
}
```

**update the AdapterImpl**

```rust
Assistant {content, extra} => {
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
            "content": Value::Null,
            "tool_calls" : tc_json_v,							
        }));	
    }
    else {
        let MessageContent::Text(content) = content;
        messages.push(json! ({"role": "assistant", "content": content}));
    }
},
```

**Update the test**

```rust
loop {
    let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;

    let mut followup_msgs:Option<Vec<ChatMessage>> = None;

    match chat_res.payload {
        ChatResponsePayload::Content(opt_mc) => {
            println!("-------------------");
            println!("Got Response\n\n");
            println!("{}", opt_mc
                .as_ref()
                .and_then(MessageContent::text_as_str)
                .unwrap_or("NO ANSWER"));
        },
        ChatResponsePayload::ToolCall(opt_tc) => {            
            if let Some(tc_vec) =  opt_tc {
                println!("-------------------");
                println!("Responding to tool_calls\n\n {:?}", &tc_vec);

                // OpenAI requires that a tool_call response be added back to the chat.
                // Without this, it will reject the subsequent "role=tool" msg.
                let vec = followup_msgs.get_or_insert(Vec::new());                            
                let tool_call_msg = ChatMessage::assistant_with_extra(
                    "".to_string(), 
                    Some(MessageExtra::ToolCall(tc_vec.clone())));
                vec.push(tool_call_msg);

                for tool_call in &tc_vec {
                    // Fake it for now. Simply return 75F
                    let tool_response_msg = ChatMessage::tool(
                        tool_call.tool_call_id.clone(), 
                        tool_call.function.fn_name.clone(),
                        "75F".to_string());
                                                                                    
                    vec.push(tool_response_msg);                        
                }

                println!("-------------------");
                println!("Adding tool_call response msg\n\n {:?}", &followup_msgs);
            }
        }
    }    

    // Continue chat as long as we have followup messages
    if let Some(msgs) = followup_msgs {
        for msg in msgs {
            chat_req = chat_req.append_message(msg);
        }            
    } else {
        break;
    }
}
```

Now that I have the end-to-end working, I renamed the 

# Too high expectations for gpt-mini ?

The `TemperatureUnits` json arg has a helpful hint embedded in it's desc. This was meant to have the LLM decide that `San Jose, CA` likes it's remperature units in `F`.

```rust
pub struct GetCurrentWeatherParams {    
    /// The city and state, e.g. San Francisco, CA
    pub location: String,

    /// The temperature unit to use. Infer this from the location.
    pub format : TemperatureUnits,
}
```

However, `gpt-4o-mini` follows the prompt up with **What temperature unit would you like the weather in? Celsius or Fahrenheit?**. Hard to believe that the model got this wrong. Maybe the system prompt of **"Don't make assumptions about what values to plug into functions. Ask for clarification if a user request is ambiguous."** made it more conservative. Maybe some other mistake on my part ?

> **Note**: While `chat_req.add_tool(..)` and `chat_req.add_message()` might make it look like the order in which we add tools and messages matter. Ultimately this is all assembled into a json with the keys `{ "tools" : .., "messages" : ...}`. The API end-point would be expected to assemble the tools before the chat messages in it's low-level prompts to the model. Within messages order should matter but between messages and tools, there should be no ordering issues.

So now, I needed a way to respond to this **What temperature unit would you like the weather in? Celsius or Fahrenheit?** within the example script. [2-AutomatedPromptResponse](./2-AutomatedPromptResponse.md) documents how I experimented with using a LLM to responsd to that question and ultimately get the tools involved after all the automated clarification.
