#![allow(dead_code)]
use super::{Musig2Instance, Outgoing};
use crate::cli::party::{
    async_protocol, async_protocol::AsyncProtocol, instance::ProtocolMessage,
    traits::state_machine::*, watcher::StderrWatcher,
};
use futures::stream::FusedStream;
use libp2p::{Multiaddr, PeerId};
use log::info;
use std::{fmt::Debug, pin::Pin};
use tokio::sync::{broadcast, broadcast::error::RecvError};

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
    // pub tx: broadcast::Sender<Msg<ProtocolMessage>>,
    /// Interact through communication channels
    // pub rx_session: broadcast::Receiver<Msg<ProtocolMessage>>,
    // pub tx_session: broadcast::Sender<Msg<ProtocolMessage>>,
    /// Parties running a protocol
    ///
    /// Field is exposed mainly to allow examining parties state after simulation is completed.
    pub parties: Vec<Multiaddr>,
    pub peer_ids: Vec<PeerId>,
    /// A single session can only be run once
    exhausted: bool,
}

impl AsyncSession {
    // /// Creates new session
    // pub fn new(session_id: MSessionId) -> Self {
    //     let (tx, _) = broadcast::channel(20);
    //     Self {
    //         session_id,
    //         runtime: Default::default(),
    //         parties: vec![],
    //         peer_ids: vec![],
    //         exhausted: false,
    //     }
    // }

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
            // tx: (),
            // rx_session: (),
            // tx_session: (),
        }
    }

    // pub fn get_runtime(&self) -> Runtime {
    //     let runtime = RefCell::new(self.runtime);
    //     runtime.into_inner()
    // }

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
            // Err(AsyncSessionError::SessionExhausted)
            info!("AsyncSessionError::SessionExhausted");
        }
        self.exhausted = true;
        println!("session {:?} running", self.session_id);
        // let mut runtime = self.get_runtime();

        // tokio::spawn(async move {
        // runtime.run().await;
        // if let ret = match self.runtime.run().await {
        //     Ok(output) => {
        //         info!("Runtime result is {:?}", output);
        //         Ok(output)
        //     },
        //     Err(err) => Err(
        //         AsyncSessionError::ProtocolExecutionError(err)
        //     ),
        // };
        // {
        //     info!("!!!!!!!! ret is {:?}", ret);
        //     ret
        // }
        // });

        // if let r = match h.await {
        //     Ok(output) => Ok(output),
        //     // Ok(Err(err)) => Err(AsyncSessionError::ProtocolExecution(err)),
        //     // Err(err) => Err(AsyncSessionError::ProtocolExecutionPanicked(err)),
        // };
        let ret = self.runtime.run().await;
        if let Ok(result) = ret {
            info!("Runtime result is {:?}", result);
            // print!("print: Runtime result is {:?}", result);
        }
    }
}

/// Possible errors that async simulation can be resulted in
#[non_exhaustive]
#[derive(Debug)]
pub enum AsyncSessionError<SM: StateMachine> {
    /// Protocol execution error
    ProtocolExecutionError(
        async_protocol::Error<
            SM::Err,
            broadcast::error::RecvError,
            broadcast::error::SendError<Msg<SM::MessageBody>>,
        >,
    ),
    /// Protocol execution produced a panic
    ProtocolExecutionPanicked(tokio::task::JoinError),
    /// Session ran twice
    SessionExhausted,
}
