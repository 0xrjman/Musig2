//! The structure of parties, Per party is a StateMachine

use traits::message::MessageContainer;

mod broadcast;
mod store_err;
mod traits;
mod rounds;
mod party;
mod sim;

pub type Store<C> = <C as MessageContainer>::Store;
