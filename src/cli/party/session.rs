#![allow(dead_code)]

use libp2p::{Multiaddr, PeerId};
use log::info;
use tokio::sync::broadcast::error::RecvError;
use std::collections::HashMap;
use std::fmt::Debug;
use std::pin::Pin;
use futures::sink::Sink;
use std::task::{Context, Poll};

use crate::cli::party::traits::state_machine::*;
use futures::future::ready;
use futures::stream::{FusedStream, StreamExt};
use crate::cli::party::sim::benchmark::Benchmark;
pub use crate::cli::party::sim::benchmark::{BenchmarkResults, Measurements};
use crate::cli::party::instance::ProtocolMessage;
use crate::cli::party::async_protocol::AsyncProtocol;
use crate::cli::party::{async_protocol, watcher::StderrWatcher};

use super::{Incoming, Musig2Instance, Outgoing};
use tokio::sync::broadcast;

pub type MSessionId = String;
pub type Runtime = AsyncProtocol<Musig2Instance, Pin<Box<dyn FusedStream<Item = std::result::Result<Msg<ProtocolMessage>, RecvError>> + Send>>, Outgoing<ProtocolMessage>, StderrWatcher>;
pub const MSESSION_ID: &str = "test";

pub struct MSession {
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
}

impl MSession {
    pub fn with_fixed_instance(session_id: MSessionId, runtime: Runtime, parties: Vec<Multiaddr>, peer_ids: Vec<PeerId>) -> MSession {
        Self{
            session_id,
            runtime,
            // tx: (),
            // rx_session: (),
            // tx_session: (),
            parties,
            peer_ids,
        }
    }
    pub async fn run(&mut self) {
        println!("session {:?} running", self.session_id);
        
        let ret = self.runtime.run().await;
        if let Ok(result) = ret {
            info!("Runtime result is {:?}", result);
            // print!("print: Runtime result is {:?}", result);
        }
    }
}