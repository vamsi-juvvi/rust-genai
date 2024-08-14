use crate::support::value_ext::ValueExt;

use schemars::JsonSchema;
use schemars::gen::{SchemaGenerator, SchemaSettings};

use serde_json::{json, Value};
use tracing::debug;

/// The schemars schema-generator, by default, wants to 
/// seprate the definitions for enums. Using the weather example
/// in the OpenAI cookbook, this looks like this.
/// 
/// "definitions": {
///  "TemperatureUnits": {
///      "enum": [
///        "Celcius",
///        "Farenheit"
///      ],
///      "type": "string"
///    }
///  },
///  "properties": {
///    "format": {
///      "allOf": [
///        {
///          "$ref": "#/definitions/TemperatureUnits"
///        }
///      ],
///      "description": "The temperature unit to use. Infer this from the users location."
///    },  
///  },
/// 
/// Customizing it to inline the so-called sub-schemas results in 
/// the following form which OpenAI accepts.
/// 
///  "properties": {
///    "format": {
///        "description": "The temperature unit to use. Infer this from the users location.",      
///         "enum": [ "Celcius", "Farenheit"],
///        "type": "string"
///      },
fn schema_generator_for_tool() -> SchemaGenerator {
    let settings = SchemaSettings::default().with(|s| {        
        s.inline_subschemas = true;
    });

    settings.into_generator()
}

/// Generate JSON schema for a tool function usable in LLM Chats
/// The function is of the following form.
///    /// $fn_desc
///    $fn_name(param:TParam)
/// 
/// The single param must derive from JsonSchema like
/// 
/// use schemars::JsonSchema;
/// #[derive(JsonSchema)]
/// struct MyParam {..}
/// 
/// Each field should have it's own `/// doc` which has to be meaningful 
/// to the LLM and will be included in the schema as `description:`
pub fn schema_for_fn_single_param<TParam>(fn_name:String, fn_desc:String) -> Value
where 
    TParam : JsonSchema
{
    let gen = schema_generator_for_tool();

    // Generate schema of the struct itself    
    let param_schema = gen.into_root_schema_for::<TParam>();
    let mut param_schema_json = serde_json::to_value(param_schema).unwrap_or_default();    
    debug!("{:<12} - Schemars for {}\n{}", 
        "schema_for_fn_single_param",
        std::any::type_name::<TParam>(),
        serde_json::to_string_pretty(&param_schema_json).unwrap());        

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
