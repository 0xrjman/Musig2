use super::{AsyncSession, MSessionId, Musig2Instance, MSESSION_ID};
use crate::cli::party::{
    async_protocol::AsyncProtocol,
    instance::ProtocolMessage,
    sim::benchmark::{Benchmark, BenchmarkResults},
    traits::state_machine::*,
    watcher::StderrWatcher,
};
use futures::{
    future::ready,
    sink::Sink,
    stream::{FusedStream, StreamExt},
};
use libp2p::{Multiaddr, PeerId};
use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::broadcast;

pub type Incoming<M> =
    Pin<Box<dyn FusedStream<Item = Result<Msg<M>, broadcast::error::RecvError>> + Send>>;

pub struct Outgoing<M> {
    pub sender: broadcast::Sender<Msg<M>>,
}

impl<M> Sink<Msg<M>> for Outgoing<M> {
    type Error = broadcast::error::SendError<Msg<M>>;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Msg<M>) -> Result<(), Self::Error> {
        self.sender.send(item).map(|_| ())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

pub fn incoming<M: Clone + Send + Unpin + 'static>(
    mut rx: broadcast::Receiver<Msg<M>>,
    me: u16,
) -> Incoming<M> {
    let stream = async_stream::stream! {
        loop {
            let item = rx.recv().await;
            yield item
        }
    };
    let stream = StreamExt::filter(stream, move |m| {
        ready(match m {
            Ok(m) => m.sender != me && (m.receiver.is_none() || m.receiver == Some(me)),
            Err(_) => true,
        })
    });
    Box::pin(stream)
}

pub type Sender = broadcast::Sender<Msg<ProtocolMessage>>;
pub type Receiver = broadcast::Receiver<Msg<ProtocolMessage>>;

pub struct Musig2Party<P> {
    pub tx_node: broadcast::Sender<Msg<ProtocolMessage>>,
    pub rx_node: broadcast::Receiver<Msg<ProtocolMessage>>,
    pub instances: HashMap<
        MSessionId,
        AsyncSession,
        // Option<AsyncProtocol<Musig2Instance, Incoming<ProtocolMessage>, Outgoing<ProtocolMessage>, StderrWatcher>>
    >,
    // Parties running a protocol
    // Field is exposed mainly to allow examining parties state after simulation is completed.
    pub parties: Vec<P>,
    pub peer_ids: Vec<PeerId>,
    benchmark: Benchmark,
}

impl Musig2Party<Multiaddr> {
    // Remove protocol participant
    pub fn remove_party(&mut self, party: Multiaddr) -> &mut Self {
        self.parties
            .remove(self.parties.iter().position(|x| *x == party).unwrap());
        self
    }

    #[allow(dead_code)]
    pub fn add_instance(&mut self, instance: Musig2Instance, receive: Receiver) {
        // Receive messages from behaviour
        let incoming = incoming(receive, instance.party_ind());
        // Send message to behaviour
        let outgoing = Outgoing {
            sender: self.tx_node.clone(),
        };
        // Create a async instance which can run as musig2
        let async_instance =
            AsyncProtocol::new(instance, incoming, outgoing).set_watcher(StderrWatcher);

        // Create a session that can execute musig2
        let msession = AsyncSession::with_fixed_instance(
            MSESSION_ID.to_string(),
            async_instance,
            self.parties.clone(),
            self.peer_ids.clone(),
        );
        // self.instance = Arc::new(Mutex::new(Some(instance)));
        self.instances.insert(MSESSION_ID.to_string(), msession);

        self.asset_dev();
    }
}

impl<P> Musig2Party<P> {
    /// Creates new Musig2Party
    pub fn new(send: Sender, receive: Receiver) -> Self {
        Self {
            tx_node: send,
            rx_node: receive,
            // instance: Arc::new(Mutex::new(None)),
            instances: HashMap::new(),
            parties: vec![],
            peer_ids: vec![],
            benchmark: Benchmark::disabled(),
        }
    }

    /// For temporary use, it can be deleted later.
    fn asset_dev(&self) {
        if self.instances.contains_key("test") {
            println!(
                "Currently in a [test] environment, numbers of instances is {:?}",
                self.instances.len()
            );
        }
    }

    /// Add protocol participant
    pub fn add_party(&mut self, party: P) -> &mut Self {
        self.parties.push(party);
        self
    }

    /// Add peer id
    pub fn add_peer_id(&mut self, peer_id: PeerId) -> &mut Self {
        self.peer_ids.push(peer_id);
        self
    }

    /// Remove peer id
    pub fn remove_peer_id(&mut self, peer_id: PeerId) -> &mut Self {
        self.peer_ids
            .remove(self.peer_ids.iter().position(|x| *x == peer_id).unwrap());
        self
    }

    #[allow(dead_code)]
    /// Enable benchmarks so they can be [retrieved](Simulation::benchmark_results) after simulation
    /// is completed
    pub fn enable_benchmarks(&mut self, enable: bool) -> &mut Self {
        if enable {
            self.benchmark = Benchmark::enabled()
        } else {
            self.benchmark = Benchmark::disabled()
        }
        self
    }

    #[allow(dead_code)]
    /// Return benchmark results if they were [enabled](Simulation::enable_benchmarks)
    ///
    /// Benchmarks show how much time (in average) [proceed](StateMachine::proceed) method takes for
    /// proceeding particular rounds. Benchmarks might help to find out which rounds are cheap to
    /// proceed, and which of them are expensive to compute.
    pub fn benchmark_results(&self) -> Option<&BenchmarkResults> {
        self.benchmark.results()
    }
}
