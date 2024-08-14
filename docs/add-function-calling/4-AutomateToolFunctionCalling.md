# Respond to Tool Calls

Once you declare the tools to an LLM and the LLM requests that those tools be called, we need to close the loop and execute the functions backing the tools.

Since I already have a struct which stores the **tool_call** response from the chat API, I can 
 - de-serialize the stringified json it holds into the struct argument to the function
 - call the function

## Deserialize tool_call args into struct

This is partially done by the `to_chat_response` function where it stores each `tool_call` instance into a `AssistantToolCall`. See the creation of the `let tool_calls` line below.

```rust
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
```

## The function parameter structs and serialization

Recall that the `c06` example uses the following

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub enum TemperatureUnits {
    Celcius,
    Farenheit
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCurrentWeatherParams {    
    /// The city and state, e.g. San Francisco, CA
    pub location: String,

    /// The temperature unit to use. Infer this from the users location.
    pub format : TemperatureUnits,
}

pub fn get_current_weather(params:GetCurrentWeatherParams) -> Result<String> {
    debug!("{:<12} - Called with params: {:?}", "c06 - get_current_weather", params);

    // return hardcoded values for now.
    let res = match params.format {
        TemperatureUnits::Celcius => "24",
        TemperatureUnits::Farenheit => "75"
    }.to_string();

    Ok(res)
}
```

and the `AssistantToolCall` and family, look like this

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

## Make an actual function call

> The function call itself can be mediated via a HashMap of function objects. For now, since it is just one function, it can be a `if`. 

The final `Result<GetCurrentWeatherParams>.map(get_current_weather)` performs the actual function call. Note that `AssistantToolCall.fn_arguments` is stringified json and leands itself to deserialization via `map` calls. The first `clone` operation is for `Option.clone` so should be acceptable in all cases.

```rust
if &tool_call.function.fn_name == "get_current_weather" {
    // Pull out the AssistantToolCall.fn_arguments field
    let fn_result = tool_call.function.fn_arguments.as_ref()                                
        // Option<Value> -> Option<Result<Value>>
        .map(|v| serde_json::from_value::<GetCurrentWeatherParams>(v.clone()))

        // propagate fn_arguments deserialization error
        // Option< Result<GetCurrentWeatherParams>> -> Result< Result<GetCurrentWeatherParams> Err1>
        // and immediately propagate Err1
        .ok_or(Error::ToolCallArgsFailedSerialization)? 

        // Result<GetCurrentWeatherParams, DeErr> -> Result<Result<String, SerErr>, FuncErr>
        // Propoagate FuncErr
        // Propagate  DeserializationErr
        .map(get_current_weather)??;                                


    // Create the tool msg with function call result.
    let tool_response_msg = ChatMessage::tool(
        tool_call.tool_call_id.clone(), 
        tool_call.function.fn_name.clone(),
        fn_result);
    
    debug!("{:<12} - Adding tool_call response {:?}", "c06", &tool_response_msg);

    vec.push(tool_response_msg);
}
```

 - long transformation chain with short-circuits on various errors
   - json Value -> Function arg, in this case, `GetCurrentWeatherParams` and propagage the deserialization error
   - Call the `get_current_weather` function and propagage it's error
 - Pack function call response into a new `role=tool` message.

The actual function call looks like below which handles the units but still hardcodes the values.

```rust
pub fn get_current_weather(params:GetCurrentWeatherParams) -> Result<String> {
    debug!("{:<12} - Called with params: {:?}", "c06 - get_current_weather", params);

    // return hardcoded values for now.
    let res = match params.format {
        TemperatureUnits::Celcius => "24",
        TemperatureUnits::Farenheit => "75"
    }.to_string();

    Ok(res)
}
```