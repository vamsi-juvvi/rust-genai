// region:    --- Modules

mod chat_options;
mod chat_req;
mod chat_res;
mod chat_stream;
mod message_content;
pub mod tool;

// -- Flatten
pub use chat_options::*;
pub use chat_req::*;
pub use chat_res::*;
pub use chat_stream::*;
pub use message_content::*;

pub mod printer;

// endregion: --- Modules
