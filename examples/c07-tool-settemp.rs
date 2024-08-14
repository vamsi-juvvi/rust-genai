use genai::chat::{ChatMessage, ChatRequest, ChatResponsePayload, MessageContent};
use genai::chat::tool::{invoke_no_args, invoke_with_args, schema_for_fn_no_param, schema_for_fn_single_param};
use genai::Client;
use std::sync::{Mutex, OnceLock};

use derive_more::From;
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

// Works great. Calls get_current_temp first and then set_current_temp
const MODEL: &str = "gpt-4o-mini"; // ✔️

// llama3-groq-8b-8192-tool-use-preview simply calls 
// set_current_temperature("5")
// const MODEL: &str = "llama3-groq-8b-8192-tool-use-preview"; // ❌

// llama3-groq-70b-8192-tool-use-preview is better and worse
// It says "I do not have the capability"
//const MODEL: &str = "llama3-groq-70b-8192-tool-use-preview"; // ❌

// Thermostat and functions ------------------------
// The functions manipulate the Thermostat singleton instance
pub fn thermostat() -> &'static Mutex<Thermostat> {
	static INSTANCE: OnceLock<Mutex<Thermostat>> = OnceLock::new();

	INSTANCE.get_or_init(|| {
        let t = Thermostat{temp : "70".to_string() };
        debug!("{:<12} - Initialized to {:?}", "c07 - thermostat()", t.temp);
        Mutex::new(t)
	})
}

// Simplify and keep the val as string instead of parsing float from strings,
// dealing with units etc.
pub struct Thermostat {
	// -- Db
	pub temp: String,
}

impl Thermostat {
    pub fn set_temperature(&mut self, t: String) {
        self.temp = t;
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetTemperatureParams {    
    /// The temperature value to set to. This will be a plain number between -100 and 100. 
    /// The number will not have any units.
    pub temperature: String,
}

pub fn get_current_temperature() -> Result<String> {

    let t = thermostat().lock().unwrap().temp.clone();
    debug!("{:<12} - returning {} ", "c06 - get_current_temperature", t);
    Ok(t)
}

pub fn set_current_temperature(params:SetTemperatureParams) -> Result<String> {
    debug!("{:<12} - Called with params: {:?}", "c06 - set_current_temperature", params);    
    thermostat().lock().unwrap().set_temperature(params.temperature);
    Ok(thermostat().lock().unwrap().temp.clone())
}


//-- errors --------------------------------------------------------------------------------
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    BotResponseIsEmpty,
    BotResponseIsUnexpected {expected: String, got : String},

    #[from]
    GenAI(genai::Error),

    #[from]
    SerdeJson(serde_json::Error)
}

impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}

///---------------------------------------------------------------------------------
/// Demonstrates an IOT scenario handled by a LLM via tool calls. Requires LLM to
/// understand callng tools sequentially in a
/// 
/// ✔️ OpenAI (Single tool call)
/// ⬜ OpenAI (Parallel tool call)
/// ❌ Groq   (Single tool call)
/// ⬜ Groq   (Parallel tool call)
#[tokio::main]
async fn main() -> core::result::Result<(), Box<dyn std::error::Error>> {    
    
    // tracing
    tracing_subscriber::fmt()
		.without_time() // For early local development.
		.with_target(false)
		.with_env_filter(
			EnvFilter::try_from_default_env()
			.unwrap_or_else(|_| EnvFilter::new("debug")))
		.init();
    

	let client = Client::default();        	

    // Setup the tools -----------------------------------
    // Generate the tool schema
    // - from the definition of GetCurrentWeatherParams
    // - plus name/desc of function.
    let sct_tool_schema = schema_for_fn_single_param::<SetTemperatureParams>(
        "set_current_temperature".to_string(), 
        "Set the current temperature".to_string(),
    );

    let gct_tool_schema = schema_for_fn_no_param(
        "get_current_temperature".to_string(), 
        "Get the current temperature".to_string(),
    );

    debug!("{:<12} -  {}", "c07 - set_current_temperature tool schema", serde_json::to_string_pretty(&sct_tool_schema).unwrap());    
    debug!("{:<12} -  {}", "c07 - get_current_temperature tool schema", serde_json::to_string_pretty(&gct_tool_schema).unwrap());    

    let mut chat_req = ChatRequest::default().with_system(
        "Don't make assumptions about what values to plug into functions. Ask for clarification if a user request is ambiguous. Use the supplied tools in the correct order to get the needed information.")
        .append_tool(gct_tool_schema)
        .append_tool(sct_tool_schema)
        .append_message(ChatMessage::user("Increase the temperature by 5 degrees"));
        
    loop {
        let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;
        
        let mut followup_msgs:Option<Vec<ChatMessage>> = None;
        
        match chat_res.payload {
            ChatResponsePayload::Content(opt_mc) => {

                // Add the assistant response to the chat_req.
                // Don't add this to the followup_msgs since we don't want this 
                // triggering a loop continuation.
                if let Some(mc) = opt_mc.clone() {
                    chat_req = chat_req.append_message(ChatMessage::assistant(mc.clone()));
                }

                let resp = opt_mc
                .as_ref()
                .and_then(MessageContent::text_as_str)
                .unwrap_or("NO ANSWER");

                debug!("{:<12} -  {}", "c07 - processing payload", resp);                
            },
            ChatResponsePayload::ToolCall(opt_tc) => {            
                if let Some(tc_vec) =  opt_tc {                    
                    debug!("{:<12} -  {:?}", "c07 - Responding to tool_calls", &tc_vec);
                    
                    // Init for followup messages
                    let vec = followup_msgs.get_or_insert(Vec::new());

                    // Add the tool_calls to the conversation before adding the response
                    vec.push(tc_vec.clone().into());

                    for tool_call in &tc_vec {

                        info!("{:<12} - Handling tool_call req for {}", "c07", tool_call.function.fn_name);

                        if &tool_call.function.fn_name == "set_current_temperature" {
                                                        
                            let fn_result = invoke_with_args(
                                set_current_temperature,
                                tool_call.function.fn_arguments.as_ref(),
                                &tool_call.function.fn_name);


                            // Create the tool msg with function call result.
                            let tool_response_msg = ChatMessage::tool_response(
                                tool_call.tool_call_id.clone(), 
                                tool_call.function.fn_name.clone(),
                                fn_result);
                            
                            debug!("{:<12} - Adding tool_call response {:?}", "c07", &tool_response_msg);

                            vec.push(tool_response_msg);
                        }
                        else if &tool_call.function.fn_name == "get_current_temperature" {
                            
                            let fn_result = invoke_no_args(get_current_temperature, &tool_call.function.fn_name);

                            // Create the tool msg with function call result.
                            let tool_response_msg = ChatMessage::tool_response(
                                tool_call.tool_call_id.clone(), 
                                tool_call.function.fn_name.clone(),
                                fn_result);
                            
                            debug!("{:<12} - Adding tool_call response {:?}", "c07", &tool_response_msg);

                            vec.push(tool_response_msg);
                        }                        
                    }                    
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
	

	Ok(())
}
