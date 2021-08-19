//! The structure of parties, Per party is a StateMachine
pub mod async_protocol;
mod broadcast;
pub mod musig2_instance;
pub mod musig2_party;
mod rounds;
pub mod session;
mod sim;
mod store_err;
pub mod traits;
pub mod watcher;

pub type Store<C> = <C as traits::message::MessageContainer>::Store;
pub use musig2_instance::*;
pub use musig2_party::*;
pub use session::*;
