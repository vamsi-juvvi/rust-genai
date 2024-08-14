# Automate tool schema generation


## The starting point

I used https://cookbook.openai.com/examples/how_to_call_functions_with_chat_models as the example. There are two tools referenced in it `get_current_weather` and `get_n_day_weather_forecast` and they are described as shown below.

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

The OpenAI docs as of July 2024 simply state that _The json should be legal [JSON Schema](https://json-schema.org/specification)_. Someone steeped in JSON schema and validating json blocks against given schemas might grok it right away. For me though, it was an invitation to go explore.

## The ending point

The rust function backing the tool definition and it's params take the following form.

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

These are used to generate the `OpenAI` compatible tool functions this way. Note that the function name and it's description are manually generated. I can imagine doing this via another macro but am not sure. Points to consider:
 - Rust function can be a generic function
 - The LLM's understanding of the tool's semantics is purely from the natural-language aspects of the tool schema: The names of the functions and parameters and the descriptions. I can see that the same rust function may be targetable to multiple LLM tool-functions by simply changing what names and descriptions we send to the API.
 - Ideally we have a rust tool-function, tool-params that are specific to the LLM semantics we want to employ. If that tool-function then decides to call another low-level rust function, that is an implementation detail.

```rust
// Generate the schema shown in the comments above 
// - from the definition of GetCurrentWeatherParams
// - plus name/desc of function.
let gcw_tool_schema = schema_for_fn_single_param::<GetCurrentWeatherParams>(
    "get_current_weather".to_string(), 
    "Get the current weather".to_string(),
);

debug!("{:<12} -  {}", "c06 - get_current_weather tool schema", serde_json::to_string_pretty(&gcw_tool_schema).unwrap());    
chat_req = chat_req.append_tool(
    gcw_tool_schema        
);
```

## Schema generation helpers

Fortunately, searching around got me to [schemars](https://crates.io/crates/schemars/0.6.0/dependencies). Unfortunately, this is at `0.6.0`, a lib which is not yet at `1.0` is always a worry. Worth a shot though. 

`schemars` will construct JSON schemas from rust structs. I need the json schemas of a function! To enable the use of `schemars`, I can use the following convention:
 - Each tool function is a rust function that takes 0 or 1 params and returns a `Result<String>`. _Note that the return should be the string form of the things the LLM understands: (text, number, json, etc)_
 - If it takes a param, it must be a single struct which derives from `schemars::JsonSchema`
 - Use `schemars` to generate the schema for the struct and take the bits that are needed
 - Assemble the function schema from manually supplied function details and the automated param schema from `schemars`


### Exploring schemars

When I use the following rust struct:

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
```

and use the following code to generate the schema (_See their [official examples](https://crates.io/crates/schemars/0.6.0)_)

```rust
use schemars::{schema_for, JsonSchema};
let schema = schema_for!(MyStruct);
```

I get the following schema.

```json
 {
  "$schema": "http://json-schema.org/draft-07/schema#",
  "definitions": {
    "TemperatureUnits": {
      "enum": [
        "Celcius",
        "Farenheit"
      ],
      "type": "string"
    }
  },
  "properties": {
    "format": {
      "allOf": [
        {
          "$ref": "#/definitions/TemperatureUnits"
        }
      ],
      "description": "The temperature unit to use. Infer this from the users location."
    },
    "location": {
      "description": "The city and state, e.g. San Francisco, CA",
      "type": "string"
    }
  },
  "required": [
    "format",
    "location"
  ],
  "title": "GetCurrentWeatherParams",
  "type": "object"
}
```

There are some differences here
 - This is a complete schema. Maybe some of the `$schema` need to be taken out before sending to OpenAI
 - `format` which is an enum has a reference to a definition listed elsewhere. Looks very different from the `OpenAI` example. If `OpenAI` says legal JSON schema it might work but who knows.

So did some json manipulations using `genai`'s existing utils to come up with this. Ignore the `schema_generator_for_tool` for now but focus on the json manipulations that transforms `schemars` generated schema to something `OpenAI` seems to want.

```rust
/// Generate JSON schema for a tool function usable in LLM Chats
/// The function is of the following form
///    $fn_name(param: $TParam)
pub fn schema_for_fn_single_param<TParam>(fn_name:String, fn_desc:String) -> Value
where 
    TParam : JsonSchema
{
    let gen = schema_generator_for_tool();

    // Generate schema of the struct itself    
    let param_schema = gen.into_root_schema_for::<TParam>();
    let mut param_schema_json = serde_json::to_value(param_schema).unwrap_or_default();    
    debug!("{:<12} -  {}", "schema_for_fn_single_param",serde_json::to_string_pretty(&param_schema_json).unwrap());

    // Insert the struct's schema into the required function schema as if the 
    // function were taking an instance of the struct.    
    let mut tool_schema_json = schema_for_fn_no_param(fn_name, fn_desc);

    tool_schema_json.x_insert(
        "/function/parameters", 
        json!({
            "type" : "object",
            "properties" : param_schema_json.x_take("/properties").unwrap_or(Value::Null),
            "required" : param_schema_json.x_take::<Value>("/required").unwrap_or(Value::Null)
        })).unwrap();        
    
    tool_schema_json
}

/// Generate JSON schema for a tool function usable in LLM Chats
/// The function is of the following form
///    $fn_name()
pub fn schema_for_fn_no_param(fn_name:String, fn_desc:String) -> Value
{    
    // Groq does not allow `"parameters" : Value::Null,`
    // while OpenAI is ok with it.
    json!({
        "type": "function",
        "function" : {
            "name" : fn_name,
            "description" : fn_desc,
            //"parameters" : Value::Null,
            },        
    })
}
```

### Schemars sub-schemas

Unfortunately, the schema that schemars generated was rejected by OpenAI. The only differene is the reference section.

```diff
 {  
+  "definitions": {
+    "TemperatureUnits": {
+      "enum": [
+        "Celcius",
+        "Farenheit"
+      ],
+      "type": "string"
+    }
+  },
  "properties": {
    "format": {
-      "type": "string",
-      "enum": ["celsius", "fahrenheit"],
+      "allOf": [
+        {
+          "$ref": "#/definitions/TemperatureUnits"
+        }
+      ],
      "description": "The temperature unit to use. Infer this from the users location."
    },
    "location": {
      "description": "The city and state, e.g. San Francisco, CA",
      "type": "string"
    }
  },
  "required": [
    "format",
    "location"
  ],  
  "type": "object"
}
```

Luckily, this did not need major changes or a yak-shaving trip to fork and change schemars. Some exploration of their docs took me to a config that was exactly what I needed. `inline_schemas = true`!. The following change worked

```rust
/// Customizing generator to inline the so-called sub-schemas results in 
/// json which OpenAI accepts.
fn schema_generator_for_tool() -> SchemaGenerator {
    let settings = SchemaSettings::default().with(|s| {        
        s.inline_subschemas = false;
    });

    settings.into_generator()
}
```

### Final OpenAI schema generation

With the above pieces in place, I start with the following `schemars` generated schema for the `c06_tool_functions::GetCurrentWeatherParams` struct.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "properties": {
    "format": {
      "description": "The temperature unit to use. Infer this from the users location.",      
      "enum": [
        "Celcius",
        "Farenheit"
      ],
      "type": "string"
    },
    "location": {
      "description": "The city and state, e.g. San Francisco, CA",
      "type": "string"
    }
  },
  "required": [
    "format",
    "location"
  ],
  "title": "GetCurrentWeatherParams",
  "type": "object"
}
```

the following call to `tool_schema::schema_for_fn_single_param`

```rust
schema_for_fn_single_param::<GetCurrentWeatherParams>(
    "get_current_weather".to_string(), 
    "Get the current weather".to_string(),
);
```

transforms it into

```json
{
  "function": {
    "description": "Get the current weather",
    "name": "get_current_weather",
    "parameters": {
      "properties": {
        "format": {
          "description": "The temperature unit to use. Infer this from the users location.",
          "enum": [
            "Celcius",
            "Farenheit"
          ],
          "type": "string"
        },
        "location": {
          "description": "The city and state, e.g. San Francisco, CA",
          "type": "string"
        }
      },
      "required": [
        "format",
        "location"
      ],
      "type": "object"
    }
  },
  "type": "function"
}
```

## Future work ideas

 Since `schemars` has not hit it's `1.0`. It might make sense to predicate it's use on a config flag.

 Might make sense to also consider a new macro that take the function and it's description (_from comments_) instead of supplying just that portion manually. Basically, replace `schema_for_fn_single_param::<GetCurrentWeatherParams>("get_current_weather".to_string(), "Get the current weather".to_string(),);` with a macro like `schema_for_fn_single_param!(get_current_weather)`.