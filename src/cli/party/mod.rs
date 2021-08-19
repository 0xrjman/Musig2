//! The structure of parties, Per party is a StateMachine
pub mod async_protocol;
mod broadcast;
pub mod instance;
pub mod musig2;
mod rounds;
pub mod session;
mod sim;
mod store_err;
pub mod traits;
pub mod watcher;

pub type Store<C> = <C as traits::message::MessageContainer>::Store;
pub use instance::*;
pub use musig2::*;
pub use session::*;
