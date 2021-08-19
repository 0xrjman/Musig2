use super::{Musig2Instance, Outgoing};
use crate::cli::party::{
    async_protocol::AsyncProtocol, instance::ProtocolMessage, traits::state_machine::*,
    watcher::StderrWatcher,
};
use futures::stream::FusedStream;
use libp2p::{Multiaddr, PeerId};
use log::info;
use std::pin::Pin;
use tokio::sync::broadcast::error::RecvError;

pub type MSessionId = String;
pub type Runtime = AsyncProtocol<
    Musig2Instance,
    Pin<Box<dyn FusedStream<Item = std::result::Result<Msg<ProtocolMessage>, RecvError>> + Send>>,
    Outgoing<ProtocolMessage>,
    StderrWatcher,
>;
pub const MSESSION_ID: &str = "test";

pub struct AsyncSession {
    pub session_id: String,
    pub runtime: Runtime,
    /// Parties running a protocol
    pub parties: Vec<Multiaddr>,
    pub peer_ids: Vec<PeerId>,

    /// Interact through communication channels
    // pub rx_session: broadcast::Receiver<Msg<ProtocolMessage>>,
    // pub tx_session: broadcast::Sender<Msg<ProtocolMessage>>,

    /// A single session can only be run once
    exhausted: bool,
}

impl AsyncSession {
    pub fn with_fixed_instance(
        session_id: MSessionId,
        runtime: Runtime,
        parties: Vec<Multiaddr>,
        peer_ids: Vec<PeerId>,
    ) -> Self {
        Self {
            session_id,
            runtime,
            parties,
            peer_ids,
            exhausted: false,
            // rx_session: (),
            // tx_session: (),
        }
    }

    /// Runs a Musig2 Execution
    ///
    /// ## Returns
    /// Returns the execution result of a party, asynchronous operation will
    /// continue until each party finish protocol (either with success or error).
    ///
    /// It's an error to call this method twice. In this case,
    /// `Err(AsyncSessionError::Exhausted)` is returned
    pub async fn run(&mut self) {
        if self.exhausted {
            info!("AsyncSessionError::SessionExhausted");
            return;
        }
        self.exhausted = true;
        println!("Session {:?} is running", self.session_id);

        let ret = self.runtime.run().await;
        if let Ok(result) = ret {
            info!("Runtime result is {:?}", result);
        }
    }
}

// todo! Implement error handling and stop running when any error occurs in Session
// Possible errors that async simulation can be resulted in
// #[non_exhaustive]
// #[derive(Debug)]
// pub enum AsyncSessionError<SM: StateMachine> {
//     /// Protocol execution error
//     ProtocolExecutionError(
//         async_protocol::Error<
//             SM::Err,
//             broadcast::error::RecvError,
//             broadcast::error::SendError<Msg<SM::MessageBody>>,
//         >,
//     ),
//     /// Protocol execution produced a panic
//     ProtocolExecutionPanicked(tokio::task::JoinError),
//     /// Session ran twice
//     SessionExhausted,
// }
