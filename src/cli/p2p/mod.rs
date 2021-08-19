//! P2P handling for musig2 nodes.
use libp2p::{floodsub::Topic, identity::Keypair, swarm::SwarmBuilder, Multiaddr, PeerId};
use std::{env, error::Error, path::PathBuf};

// pub mod addr;
pub mod behaviour;
pub mod msg;
// pub mod swarm;
pub mod transport;

// pub use addr::{MultiaddrWithPeerId, MultiaddrWithoutPeerId};
pub use behaviour::*;
pub use msg::*;
pub use transport::build_transport;
// pub use swarm::*;

use super::protocals::signature;
use crate::cli::party::{instance::ProtocolMessage, musig2::Sender, traits::state_machine::Msg};
use crate::TOPIC;
use tokio::sync::broadcast;

/// Type alias for [`libp2p::Swarm`] running the [`behaviour::Behaviour`] with the given [`SignatureBehaviour`].
pub type TSwarm = libp2p::swarm::Swarm<behaviour::SignatureBehaviour>;
/// Type alias for [`cuve::secp256k1::keypair`]
pub type Keyring = signature::KeyPair;

const DEFAULT_LISTENING_ADDRESS: &str = "/ip4/0.0.0.0/tcp/0";
const DEFAULT_TOPIC: &str = "test";

/// Defines the configuration for an musig2 swarm.
#[derive(Clone)]
pub struct SwarmOptions {
    pub store_path: PathBuf,
    /// The keypair for the PKI based identity of the local node.
    pub keypair: Keypair,
    /// The keyring for digital signature.
    pub keyring: Keyring,
    /// The peer address of the local node created from the keypair.
    pub peer_id: PeerId,
    /// The subscription topic
    pub topic: Topic,
    /// Bound listening addresses; by default the node will not listen on any address.
    pub listening_addrs: Multiaddr,
    /// Enables mdns for peer discovery and announcement when true.
    pub mdns: bool,
    /// Responsible for transferring the data flow from other parties to the state machine
    pub tx_node: broadcast::Sender<CallMessage>,
    pub tx_party: broadcast::Sender<Msg<ProtocolMessage>>,
}

impl SwarmOptions {
    /// Creates for any testing purposes.
    pub fn new_test_options(tx_node: broadcast::Sender<CallMessage>, tx_party: Sender) -> Self {
        let keypair = Keypair::generate_secp256k1();
        let keyring = Keyring::create();
        let peer_id = PeerId::from(keypair.public());
        let topic = Topic::new(DEFAULT_TOPIC);

        log::info!("Local peer id: {:?}", peer_id);

        Self {
            tx_node,
            tx_party,
            // rx: receive,
            store_path: env::temp_dir(),
            keypair,
            keyring,
            peer_id,
            topic,
            listening_addrs: DEFAULT_LISTENING_ADDRESS.parse().unwrap(),
            // listening_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            mdns: true,
        }
    }
}

/// Creates a new musig2 swarm.
pub async fn create_swarm(options: SwarmOptions) -> Result<TSwarm, Box<dyn Error>> {
    let transp = build_transport(options.clone().keypair);
    let mut behaviour: SignatureBehaviour = build_signature_behaviour(options.clone()).await;
    // Configure at startup
    behaviour.floodsub.subscribe(TOPIC.to_owned());

    let swarm = SwarmBuilder::new(transp, behaviour, options.peer_id)
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    Ok(swarm)
}

// pub fn connection_point_addr(cp: ConnectedPoint) -> Multiaddr {
//     match cp {
//         ConnectedPoint::Dialer { address } => address,
//         ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
//     }
// }
