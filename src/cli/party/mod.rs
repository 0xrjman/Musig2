//! The structure of parties, Per party is a StateMachine

use traits::message::MessageContainer;

pub mod async_protocol;
mod broadcast;
pub mod instance;
pub mod musig2;
mod rounds;
mod sim;
mod store_err;
pub mod traits;
pub(crate) mod watcher;

pub type Store<C> = <C as MessageContainer>::Store;
pub use instance::*;
pub use musig2::*;
