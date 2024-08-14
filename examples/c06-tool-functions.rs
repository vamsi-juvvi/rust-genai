use genai::chat::{ChatMessage, ChatRequest, ChatResponsePayload, MessageContent};
use genai::chat::tool::{invoke_with_args, schema_for_fn_single_param};
use genai::Client;

use derive_more::From;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

const INSTANT_MODEL: &str = "llama-3.1-8b-instant";
const MODEL: &str = "gpt-4o-mini"; // ✔️
//const MODEL: &str = "llama3-groq-8b-8192-tool-use-preview"; // ✔️

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

//-- errors --------------------------------------------------------------------------------
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    BotResponseIsEmpty,
    BotResponseIsUnexpected {expected: String, got : String},
    ToolCallArgsFailedSerialization,

    #[from]
    GenAI(genai::Error),
}

impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}

//---------------------------------------------------------------------------------------
async fn llm_is_yes_no(client: &Client, context: String, question: String) -> Result<bool> {
    debug!("{:<12} - Calling groq with question :{} on context: {}", "c06 - llm_is_yes_no", question, context);

    let chat_req = ChatRequest::default()
        .with_system("You are an assistant that answers questions with a yes or no.")
        .append_message(
        ChatMessage::user(format!(
            "I have a question about the text enclosed in triple quotes.
             Please answer it with either a yes or no with no punctuation.

             question : {}
            '''
            {}
            '''", question, context)
        ));

    let chat_res = client
        .exec_chat(INSTANT_MODEL, chat_req, None)
        .await?;

    if let ChatResponsePayload::Content(opt_mc) = chat_res.payload 
    {             
        opt_mc
            .as_ref()
            .and_then(MessageContent::text_as_str)
            .map( |v| {
                    debug!("{:<12} - Processing response {:?}", "c06 - llm_is_yes_no", v);                    
                    match v.trim().to_lowercase().as_str() {
                        "yes" => Ok(true),
                        "no" => Ok(false),
                        _ => {                    
                            Err(Error::BotResponseIsUnexpected {
                                expected : "\"yes\" or \"no\"".to_string(),
                                got : v.to_string()
                            })
                        },
                    }
                })
                .unwrap_or(Err(Error::BotResponseIsEmpty))
    }
    else 
    {        
    Err(Error::BotResponseIsUnexpected{
        expected: "MessageContext response".to_string(),
        got: "Something else".to_string()})
    }

} 

///---------------------------------------------------------------------------------
/// Demonstrates the following OpenAI cookbook example for tool call.
///---------------------------------------------------------------------------------
/// https://cookbook.openai.com/examples/how_to_call_functions_with_chat_models
///
/// messages = []
/// messages.append({
///      "role": "system", 
///      "content": ("Don't make assumptions about what values to plug into" 
///                  " functions. Ask for clarification if a user request is"
///                  " ambiguous.")
/// })
/// messages.append({"role": "user", "content": "What's the weather like today"})
/// 
/// tools = [
///  {
///      "type": "function",
///      "function": {
///          "name": "get_current_weather",
///          "description": "Get the current weather",
///          "parameters": {
///              "type": "object",
///              "properties": {
///                  "location": {
///                      "type": "string",
///                      "description": "The city and state, e.g. San Francisco, CA",
///                  },
///                  "format": {
///                      "type": "string",
///                      "enum": ["celsius", "fahrenheit"],
///                      "description": "The temperature unit to use. Infer this from the users location.",
///                  },
///              },
///              "required": ["location", "format"],
///          },
///      }
///  },
///---------------------------------------------------------------------------------
/// Chat response that invokes a tool-call when it decides 
/// it needs to call the tool to answer a query.
/// ✔️ OpenAI (Single tool call)
/// ⬜ OpenAI (Parallel tool call)
/// ✔️ Groq   (Single tool call)
/// ⬜ Groq   (Parallel tool call)
#[tokio::main]
async fn main() -> core::result::Result<(), Box<dyn std::error::Error>> {    
    
    // tracing
    tracing_subscriber::fmt()
		.without_time() // For early local development.
		.with_target(false)
		.with_env_filter(
			EnvFilter::try_from_default_env()
			.unwrap_or_else(|_| EnvFilter::new("info")))
		.init();
    

	let client = Client::default();
    
	let mut chat_req = ChatRequest::default().with_system(
        "Don't make assumptions about what values to plug into functions. Ask for clarification if a user request is ambiguous."
    );    

    // Generate the schema shown in the comments above 
    // - from the definition of GetCurrentWeatherParams
    // - Manually add name/desc of function.
    let gcw_tool_schema = schema_for_fn_single_param::<GetCurrentWeatherParams>(
        "get_current_weather".to_string(), 
        "Get the current weather".to_string(),
    );

    debug!("{:<12} -  {}", "c06 - get_current_weather tool schema", serde_json::to_string_pretty(&gcw_tool_schema).unwrap());    
    chat_req = chat_req.append_tool(
        gcw_tool_schema        
    );

    // messages.append({"role": "user", "content": "What's the weather like today"})
    // Specify the name of the city to make sure it calls the `get_current_weather` tool.
    chat_req = chat_req.append_message(ChatMessage::user("What's the weather like today in San Jose, CA?"));

    // There will be some fancy logic needed to figure out how to terminate a chat loop
    //  - When is the LLM asking us for more info
    //  - When is it asking us to call a tool (this we know)
    //  - When does it have all the info neeed to finish responding to the initial query.
    //
    // Need to see if the JSON has any special field that marks it as a continuation    
    loop {
        let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;
        
        let mut followup_msgs:Option<Vec<ChatMessage>> = None;

        // This crude way of detecting followup questions about which temp
        // sometimes backfires. It needs a stack where once tool-call has been 
        // responded-to. We no longer expect it to ask clarifying questions about
        // which temp unit to use.        

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

                debug!("{:<12} -  {}", "c06 - processing payload", resp);

                let yn = llm_is_yes_no(&client, resp.to_string(),                 
                    "Is this a question asking to choose between celcius and fahrenheit?".to_string())
                    .await?;

                if yn {
                    // Randomly choose celsius as the response
                    debug!("{:<12} - Responding with Celcius to {}", "c06", resp);                    
                    followup_msgs
                    .get_or_insert(Vec::new())
                    .push(
                        ChatMessage::user("celsius")
                    );
                }
            },
            ChatResponsePayload::ToolCall(opt_tc) => {            
                if let Some(tc_vec) =  opt_tc {                    
                    debug!("{:<12} -  {:?}", "c06 - Responding to tool_calls", &tc_vec);
                    
                    let vec = followup_msgs.get_or_insert(Vec::new());

                    // OpenAI requires that the assistant's tool_call request be added back 
                    // to the chat. Without this, it will reject the subsequent "role=tool" msg.
                    vec.push(tc_vec.clone().into());

                    for tool_call in &tc_vec {

                        info!("{:<12} - Handling tool_call req for {}", "c06", tool_call.function.fn_name);

                        if &tool_call.function.fn_name == "get_current_weather" {
                        
                            let fn_result = invoke_with_args(
                                get_current_weather, 
                                tool_call.function.fn_arguments.as_ref(), 
                                &tool_call.function.fn_name);

                            // Create the tool msg with function call result.
                            let tool_response_msg = ChatMessage::tool_response(
                                tool_call.tool_call_id.clone(), 
                                tool_call.function.fn_name.clone(),
                                fn_result);
                            
                            debug!("{:<12} - Adding tool_call response {:?}", "c06", &tool_response_msg);

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
