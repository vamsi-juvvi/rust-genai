# Harden tool calling 

A final pass to fix the TODO's picked up during the iterative development.

# Error handling during tool calls

When implementing the tool-calling behavior, I had not given much thought to error handling beyond the fact that the tool functions return a `Result<String, Error>` for some `Error`. The articles read and hints picked up during the impementation pointed to the `tool_call_response` being of the type `String Response | String Error`: a semantic sum-type if you will.

Thinking about UX a bit, least-surprise would indicate
 - Chat like UX requires that tool failures actually be sent to the LLM so it can convey it to the user.
 - User will need two things
   - The fact that an error has occured
   - A reference that can be used during support
   - The right amount of verbiage so that the LLM won't be confused. _(meaning, don't dump trace info into the error)_

This then suggests something similar to a `server/client` error split
 - Log/Trace errors on the server side (_enhanced later with an id_)
 - Function to map server errors to client errors (_Much like what is done for a web-server app_)
 - Simple error info that the LLM can digest (_Error XXXX: YY tool_call failed_). Note that the tool_call_id is part of the response so the LLM will know which tool call failed.
 - A bottleneck function to manage this

**Correctness**
 - Error should be supplied to the LLM as a string

**Polish**
 - `Server -> client` error mapping
 - ID for the error for support calls and to tie server/client errors together


## invoke_tool_call_function - no params

Error handling is much simpler when the tool_function takes no params. In the _c07_ case, the `get_current_temperature` is representative. Originally, it looked like this:

```rust
let fn_result = get_current_temperature()?;
```

Changing it to satisfy our correctness requirements was straightforward.
 - ✔️ Let string value through if success
 - ✔️ Get error string if error

```rust
  let fn_result = get_current_temperature()
        .map_or_else(
            |e| format!("Error during 'get_current_temperature' {}", e.to_string()), 
            |t| t);
```

 We can later 
  - apply a `server -> client` error mapping 
  - trace and log using an `inspect` call.

## invoke_tool_call_function - params

The code in _c07_ that invoked the `set_current_temperature` looked like this:

```rust
let fn_result = tool_call.function.fn_arguments.as_ref()                                                                
        .map(|v| serde_json::from_value::<SetTemperatureParams>(v.clone()))
        .ok_or(Error::ToolCallArgsFailedSerialization)? 
        .map(set_current_temperature)??;
```

Three `?` with no error mapping.

---

**Handle the `fn_arguments` not being succesfully read from the tool_call json**

```rust
tool_call.function.fn_arguments.as_ref().map(..)
```
can be changed to 

```rust
tool_call.function.fn_arguments.as_ref().map_or_else(|| Err(..), Ok)
``` 

to convert the `Option<&Value>` to a `Result<&Value, Error>`. 

---

**Handle the serialization error**

The input to this stage is a `Result<&Value, Error>`

```rust
map(|v| serde_json::from_value::<SetTemperatureParams>(v.clone()))
```

will create a `Result < Result<SetTemperatureParams, ErrSerde > Error>`. Best to flatten it right away for better readability

`map` will nest but `and_then`, the monadic _bind_, will process the `Ok` without nesting. However, it will only go down the `Ok` branch.

```rust
and_then(|v| serde_json::from_value::<SetTemperatureParams>(v.clone()))
```

`and_then` imposes additional requirements
 - it's `Err` branch will simply propagate the `Err(Error)` received
 - it's `Ok` branch will return a `Result<.., serde_json::Error>`, a different error type. Rust requires both branch handlers to return the same type.
 - This is not Ok :-) and must be managed so that both branches return the same error type.

Enhance the error enum to wrap serde errors, like so

```diff
pub enum Error {
    BotResponseIsEmpty,
    BotResponseIsUnexpected {expected: String, got : String},

    ToolCallArgsFailedSerialization,

    #[from]
    GenAI(genai::Error),

+    #[from]
+    SerdeJson(serde_json::Error)
}
```

and map the serde_error _(I might be missing something here, unlike in `?`, I am having to do it manually bu invoking `.into()`_)


```rust
.and_then( |v| {
              serde_json::from_value::<SetTemperatureParams>(v.clone())
                  .map_err(|e| e.into())
          }
      )
```

----

The last part of the chain 

```rust
     .ok_or(Error::ToolCallArgsFailedSerialization)? 
     .map(set_current_temperature)??;
```

can again use 
 - `and_then` to continue the monadic bind along the success path
 - with a final `map_or_else` to handle both success and failure and return the string type we need.

```rust
.and_then(set_current_temperature)
.map_or_else(
    |e| format!("Error during 'get_current_temperature' {}", e.to_string()), 
    |val| val)
```

Finally, the whole thing becomes

```rust
let fn_result = tool_call.function.fn_arguments.as_ref()
    .map_or_else(|| Err(Error::ToolCallArgsFailedSerialization), Ok)
    .and_then(|v| {
            serde_json::from_value::<SetTemperatureParams>(v.clone())
                .map_err(|e| e.into())
        }
    )
    .and_then(set_current_temperature)
    .map_or_else(
        |e| format!("Error during 'get_current_temperature' {}", e.to_string()), 
        |val| val);
```

While this might require a pause to digest it. The semantics to lines-of-code ratio (`SEMLOCR`: You heard it here first!) is very high. Imagine doing all of this with `if let Some(v) = opt` and `if let Ok(v) = val` with additional `match` statements thrown in.

## Error processing in a chain - summary

The actions can be summarised as
 - initial `map_or_else` to convert `Option` to a `Result` with the correct error.
 - cascading `and_then` calls to perform the monadic transformations of the success branch
   - Convert any thirdparty errors into our Error type.
 - final `map_or_else` to convert `Ok` and `Err` to the string we need. 
   - All errors are our type
   - Either mapped from thirdparty-errors
   - or Natively via `Result<T, Error>`

## Refactoring tool function execution

Now that I have gone through the correctness mechanisms, it is relatively straigtforward to refactor this out so that a client does not have to deal with this each time.

The original calls in _c07_ changed from 

```rust
let fn_result = tool_call.function.fn_arguments.as_ref()
      .map_or_else(|| Err(Error::ToolCallArgsFailedSerialization), Ok)
      .and_then(|v| {
              serde_json::from_value::<SetTemperatureParams>(v.clone())
                  .map_err(|e| e.into())
          })
      .and_then(set_current_temperature)
      .map_or_else(
          |e| format!("Error during '{}' {}", &tool_call.function.fn_name, e.to_string()), 
          |val| val);

 ..snip...

 let fn_result = get_current_temperature()
      .map_or_else(
          |e| format!("Error during '{}' {}", &tool_call.function.fn_name, e.to_string()),
          |t| t);

```

to

```rust

use genai::chat::tool::{invoke_no_args, invoke_with_args}

let fn_result = invoke_with_args(
    set_current_temperature,
    tool_call.function.fn_arguments.as_ref(),
    "set_current_temperature");

...snip...

let fn_result = invoke_no_args(get_current_temperature, "get_current_temperature");
```

---

The tool_calls in `c06` changed from

```rust
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
```

to 

```rust
let fn_result = invoke_with_args(
      get_current_weather, 
      tool_call.function.fn_arguments.as_ref(), 
      "get_current_weather");
```

---

The invoker code was placed in a new module under `genai.chat.tool.tool_invoke`

```rust
use std::fmt::Display;

use derive_more::From;
use serde::de::DeserializeOwned;
use serde_json::Value;

//-- Errors ----------------------------------------
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {    
    ToolCallArgsFailedSerialization,
    ToolCallFunctionFailed(String),

    #[from]
    SerdeJson(serde_json::Error)
}

impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}

//----------------------------------------- Errors ----

pub fn invoke_no_args<F, E>(func:F, fn_name:&str) -> String
where 
    F: FnOnce() -> core::result::Result<String, E>,
    E: Display + std::fmt::Debug
{
    func()
        // trace info
        .inspect(|x| 
            info!("{:<12} - {:?} returned {:?}", "invoke_no_args", fn_name, x))            
        // trace error
        .inspect_err(|e| 
            error!("{:<12} - {:?} errored with {:?}", "invoke_no_args", fn_name, e))
        .map_or_else(|e| format!("Error during '{}' {}", &fn_name, e),
                     |t| t) 
}


pub fn invoke_with_args<F, A, E>(func:F, args: Option<&Value>, fn_name:&str,) -> String
where 
    A: DeserializeOwned,
    F: FnOnce(A) -> core::result::Result<String, E>,
    E: Display + std::fmt::Debug,
{
    // Convert Option<&Value> to Result<&Value, Error>
    args.map_or_else(|| Err(Error::ToolCallArgsFailedSerialization), Ok)
        // Deserialize &Value into A and return Result<A, Error>        
        .and_then(|v| {
                serde_json::from_value::<A>(v.clone())
                    .map_err(|e| e.into())
            })
        // Call func and map E to Error: returns Result<String, Error>
        .and_then(
            |args| {
                func(args)
                    .map_err(|e|Error::ToolCallFunctionFailed(format!("{}", e)))
            })
        // trace info
        .inspect(|x| 
            info!("{:<12} - {:?} returned {:?}", "invoke_with_args", fn_name, x))            
        // trace error
        .inspect_err(|e| 
            error!("{:<12} - {:?} errored with {:?}", "invoke_with_args", fn_name, e))
        // Finally handle both Ok and Err to return String
        .map_or_else(
            |e| format!("Error during '{}' {}", &fn_name, e.to_string()), 
            |val| val)
}
```

 - New errors generated by this module with `#[from]` for third-party ones like serde.
 - Since we are dealing with `LLMs` and text responses are required, the invokers return `String`s. Can genericize it to a `T` if needed but not needed right now.
 - `FnOnce() -> Result<String, E>` uses an error external to the invoker module. Since we are dealing with `String` return type, all we need is `where E : Display` and use `{}` to convert to string. If I had chosen a `T` instead of `String`, then it will need a `Into<T>` conversion for the error.
 - `inspect_err` seems reasonable here but not sure about the use of `inspect` to trace in low level code like this. Revisit.

## Errors in tool functions

I sumulated an error in one of the tool functions by using

```rust
pub fn get_current_temperature() -> Result<String> {
    Err(Error::ToolError("Unknown error".to_string()))
}
```

and I get the following in the tracing logs.

```console
EBUG c07 - Responding to tool_calls -  [AssistantToolCall { tool_call_id: "call_ywbGVQOtc4J5RvAkGFvuw1qU", tool_call_type: "function", function: AssistantToolCallFunction { fn_name: "get_current_temperature", fn_arguments: Some(Object {}) } }]
 INFO c07          - Handling tool_call req for get_current_temperature
ERROR invoke_no_args - "get_current_temperature" errored with ToolError("Unknown error")
DEBUG c07          - Adding tool_call response ToolResponse(ToolMessage { tool_call_id: "call_ywbGVQOtc4J5RvAkGFvuw1qU", tool_name: "get_current_temperature", tool_result: "Error during 'get_current_temperature' ToolError(\"Unknown error\")" })
```

Ultimately, this error results in this tool_response being sent

```console
{
    "content": "Error during 'get_current_temperature' ToolError(\"Unknown error\")",
    "name": "get_current_temperature",
    "role": "tool",
    "tool_call_id": "call_ywbGVQOtc4J5RvAkGFvuw1qU"
}
```

> OpenAI actually called the same tool again (_with a different call_id_) before concluding that it actually was an error.
and this results in this final respose from OpenAI:

```console
It seems that I'm currently unable to retrieve the current temperature. 

Could you please specify the current temperature? Once I have that information, I can help you increase it by 5 degrees.",
```