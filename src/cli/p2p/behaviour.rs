//! P2P handling for musig2 nodes.
use super::{CallMessage, Message, SwarmOptions};
use crate::cli::party::{musig2_instance::ProtocolMessage, traits::state_machine::Msg};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent},
    mdns::{Mdns, MdnsEvent},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour,
};
use log::{debug, info};

#[derive(NetworkBehaviour)]
pub struct SignatureBehaviour {
    #[behaviour(ignore)]
    options: SwarmOptions,
    pub floodsub: Floodsub,
    mdns: Mdns,
}

impl SignatureBehaviour {
    /// Create a Kademlia behaviour with the IPFS bootstrap nodes.
    pub async fn new(options: SwarmOptions) -> Self {
        info!("net: starting with peer id {}", options.peer_id);

        let floodsub = Floodsub::new(options.peer_id);
        let mdns = if options.mdns {
            Some(Mdns::new(Default::default()).await.unwrap())
        } else {
            None
        }
        .unwrap();

        SignatureBehaviour {
            options,
            floodsub,
            mdns,
        }
    }

    pub fn options(&mut self) -> &mut SwarmOptions {
        &mut self.options
    }
}

pub async fn build_signature_behaviour(options: SwarmOptions) -> SignatureBehaviour {
    SignatureBehaviour::new(options).await
}

impl NetworkBehaviourEventProcess<()> for SignatureBehaviour {
    fn inject_event(&mut self, _event: ()) {}
}

impl NetworkBehaviourEventProcess<void::Void> for SignatureBehaviour {
    fn inject_event(&mut self, _event: void::Void) {}
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for SignatureBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        if let FloodsubEvent::Message(msg) = event {
            // Pass received ProtocolMessage to node through internal channel
            if let Ok(resp) = serde_json::from_slice::<Msg<ProtocolMessage>>(&msg.data) {
                info!("received message form peers");
                self.options().tx_party.send(resp).unwrap();
            }

            if let Ok(call) = serde_json::from_slice::<CallMessage>(&msg.data) {
                info!("Receive a call from peers, {:?}", call);
                match call {
                    CallMessage::CoopSign(sign_info) => {
                        let call = CallMessage::CoopSign(sign_info);
                        self.options.tx_node.send(call).unwrap();
                    }
                }
            }

            if let Ok(resp) = serde_json::from_slice::<Message>(&msg.data) {
                match resp {
                    Message::Round1(_r1) => {
                        debug!("received round 1 message, it has been deprecated")
                    }
                    Message::Round2(_r2) => {
                        debug!("received round 1 message, it has been deprecated")
                    }
                }
            }
        };
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for SignatureBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}
