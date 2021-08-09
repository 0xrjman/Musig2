//! The structure of parties, Per party is a StateMachine

use traits::message::MessageContainer;

mod broadcast;
mod instance;
mod rounds;
mod sim;
mod store_err;
mod traits;

pub type Store<C> = <C as MessageContainer>::Store;
