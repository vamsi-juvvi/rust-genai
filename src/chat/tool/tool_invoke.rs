use std::fmt::Display;

use derive_more::From;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::{error, info};

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