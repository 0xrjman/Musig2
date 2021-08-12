//! P2P handling for musig2 nodes.
use curv::elliptic::curves::{
    secp256_k1::{FE, GE},
    traits::ECPoint,
};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent},
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour, PeerId,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tracing::trace;

use super::SwarmOptions;
use crate::cli::protocals::signature::*;
use crate::{
    C, KEYAGG, KEY_PAIR, MSG, PENDINGSTATE, PKS, R, ROUND1, ROUND2, S, SIGNSTATE, STATE0, STATE1,
    TOPIC,
};

use crate::cli::party::{traits::state_machine::Msg, instance::ProtocolMessage};


#[derive(Clone, Debug)]
pub enum SignState {
    Prepare(Message),
    Round1Send,
    Round1End(Message),
    Round2Send,
    Round2End,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum EventType {
    Response(Message),
    Input(String),
    Send(Message),
    Response1(Msg<ProtocolMessage>)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Message {
    Round1(Round1),
    Round2(Round2),
}

#[derive(NetworkBehaviour)]
pub struct SignatureBehaviour {
    #[behaviour(ignore)]
    options: SwarmOptions,
    pub floodsub: Floodsub,
    mdns: Mdns,
    ping: Ping,
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

        let ping = Ping::default();
        // let pubsub = Pubsub::new(options.peer_id);
        // let mut swarm = SwarmApi::default();

        // for (addr, _peer_id) in &options.bootstrap {
        //     if let Ok(addr) = addr.to_owned().try_into() {
        //         swarm.bootstrappers.insert(addr);
        //     }
        // }

        SignatureBehaviour {
            options,
            floodsub,
            mdns,
            ping,
        }
    }

    pub fn floodsub(&mut self) -> &mut Floodsub {
        &mut self.floodsub
    }

    pub fn options(&mut self) -> &mut SwarmOptions {
        &mut self.options
    }

    pub fn generate_round1(&self, msg: Vec<u8>) -> Message {
        let (party_1_msg_round_1, party_1_state) = sign(KEY_PAIR.clone());
        STATE0
            .lock()
            .unwrap()
            .insert(self.options.topic.clone(), party_1_state);
        PKS.lock()
            .unwrap()
            .insert(self.options.peer_id, KEY_PAIR.public_key);
        ROUND1
            .lock()
            .unwrap()
            .insert(self.options.peer_id, party_1_msg_round_1.clone());
        let round1 = Round1 {
            topic: "test".to_string(),
            sender: self.options.peer_id.clone().to_bytes(),
            num: 3,
            ephemeral_keys: party_1_msg_round_1,
            msg,
            pubkey: KEY_PAIR.public_key,
        };
        Message::Round1(round1)
    }
    pub fn solve_msg_in_generating_round_1(&mut self, msg: Vec<u8>) {
        self.floodsub.publish(
            TOPIC.to_owned(),
            serde_json::to_string(&self.generate_round1(msg))
                .unwrap()
                .as_bytes(),
        );
    }
    pub fn generate_round2(&self, e: FE) -> Message {
        let round2 = Round2 {
            topic: "test".to_string(),
            sender: self.options.peer_id.clone().to_bytes(),
            sign: e,
        };
        Message::Round2(round2)
    }

    pub async fn verify_sign(&self) {
        let r = *R.lock().unwrap().get(&self.options.topic).unwrap();
        let s = *S.lock().unwrap().get(&self.options.topic.clone()).unwrap();
        let c = C
            .lock()
            .unwrap()
            .get(&self.options.topic.clone())
            .unwrap()
            .clone();
        let key_agg = KEYAGG
            .lock()
            .unwrap()
            .get(&self.options.topic.clone())
            .unwrap()
            .clone();
        let result = verify(&s, &r.x_coor().unwrap(), &key_agg.X_tilde, &c).is_ok();
        println!("verify result:{}", result);
    }
}

/// Create a IPFS behaviour with the IPFS bootstrap nodes.
pub async fn build_signature_behaviour(options: SwarmOptions) -> SignatureBehaviour {
    SignatureBehaviour::new(options).await
}

impl NetworkBehaviourEventProcess<()> for SignatureBehaviour {
    fn inject_event(&mut self, _event: ()) {}
}

impl NetworkBehaviourEventProcess<void::Void> for SignatureBehaviour {
    fn inject_event(&mut self, _event: void::Void) {}
}

impl NetworkBehaviourEventProcess<PingEvent> for SignatureBehaviour {
    fn inject_event(&mut self, event: PingEvent) {
        use libp2p::ping::handler::{PingFailure, PingSuccess};
        match event {
            PingEvent {
                peer,
                result: Result::Ok(PingSuccess::Ping { rtt }),
            } => {
                trace!(
                    "ping: rtt to {} is {} ms",
                    peer.to_base58(),
                    rtt.as_millis()
                );
                // self.swarm.set_rtt(&peer, rtt);
            }
            PingEvent {
                peer,
                result: Result::Ok(PingSuccess::Pong),
            } => {
                trace!("ping: pong from {}", peer);
            }
            PingEvent {
                peer,
                result: Result::Err(PingFailure::Timeout),
            } => {
                trace!("ping: timeout to {}", peer);
                // self.remove_peer(&peer);
            }
            PingEvent {
                peer,
                result: Result::Err(PingFailure::Other { error }),
            } => {
                error!("ping: failure with {}: {}", peer.to_base58(), error);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for SignatureBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        if let FloodsubEvent::Message(msg) = event {
            /* todo!The event type here may need to be redefined, and then it can receive
                ProtocolMessage messages from other parties and send them to rx of the finite state machine
                through swarm.behaviour_mut().options().tx */
            if let Ok(resp) = serde_json::from_slice::<Message>(&msg.data) {
                match resp {
                    Message::Round1(r1) => {
                        println!(
                            "recv round1 peerid:{:?}",
                            PeerId::from_bytes(&r1.sender).unwrap()
                        );
                        PKS.lock()
                            .unwrap()
                            .insert(PeerId::from_bytes(&r1.sender).unwrap(), r1.pubkey);
                        ROUND1
                            .lock()
                            .unwrap()
                            .insert(PeerId::from_bytes(&r1.sender).unwrap(), r1.ephemeral_keys);
                        let state = SIGNSTATE
                            .lock()
                            .unwrap()
                            .get(&self.options.topic)
                            .unwrap()
                            .clone();
                        if let SignState::Round2End = state {
                            let resp1 = self.generate_round1(r1.msg.clone());
                            SIGNSTATE
                                .lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), SignState::Prepare(resp1));
                        };
                        let round1_num = ROUND1.lock().unwrap().len();
                        if round1_num == r1.num {
                            let mut peer_ids =
                                PKS.lock().unwrap().keys().cloned().collect::<Vec<PeerId>>();
                            peer_ids.sort();
                            let mut index = 99999;
                            let mut pks = vec![];
                            for (i, p) in peer_ids.iter().enumerate() {
                                let pk = *PKS.lock().unwrap().get(p).unwrap();
                                pks.push(pk);
                                if p.as_ref() == self.options.peer_id.as_ref() {
                                    index = i;
                                }
                            }
                            let party1_key_agg = KeyAgg::key_aggregation_n(&pks, index);
                            // println!("index:{:?}\npks:{:?}\nagg:{:?}", index, pks,party1_key_agg.clone());
                            KEYAGG
                                .lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), party1_key_agg);
                            let party1_received_msg_round_1 = ROUND1
                                .lock()
                                .unwrap()
                                .clone()
                                .into_iter()
                                .filter(|(k, _)| k.as_ref() != self.options.peer_id.as_ref())
                                .map(|(_, v)| v)
                                .collect::<Vec<Vec<GE>>>();
                            let party_1_state = STATE0
                                .lock()
                                .unwrap()
                                .get(&self.options.topic.clone())
                                .unwrap()
                                .clone();
                            let (party_1_state_prime, party1_msg_round_2) = party_1_state
                                .sign_prime(&r1.msg, &pks, party1_received_msg_round_1, index);
                            STATE1
                                .lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), party_1_state_prime);
                            PENDINGSTATE.lock().unwrap().insert(
                                self.options.topic.clone(),
                                SignState::Round1End(self.generate_round2(party1_msg_round_2)),
                            );
                        }
                    }
                    Message::Round2(r2) => {
                        println!(
                            "recv round2 peerid:{:?}",
                            PeerId::from_bytes(&r2.sender).unwrap()
                        );
                        ROUND2
                            .lock()
                            .unwrap()
                            .insert(PeerId::from_bytes(&r2.sender).unwrap(), r2.sign);
                        let round2_num = ROUND2.lock().unwrap().len();
                        if round2_num == PKS.lock().unwrap().len() - 1 {
                            let party1_received_msg_round_2 = ROUND2
                                .lock()
                                .unwrap()
                                .values()
                                .cloned()
                                .collect::<Vec<FE>>();
                            let party_1_state_prime = STATE1
                                .lock()
                                .unwrap()
                                .get(&self.options.topic.clone())
                                .unwrap()
                                .clone();
                            let s_total_1 = sign_double_prime(
                                party_1_state_prime,
                                &party1_received_msg_round_2,
                            );
                            let party_1_state = STATE0
                                .lock()
                                .unwrap()
                                .get(&self.options.topic.clone())
                                .unwrap()
                                .clone();
                            let msg = MSG
                                .lock()
                                .unwrap()
                                .get(&self.options.topic.clone())
                                .unwrap()
                                .clone();
                            let mut peer_ids =
                                PKS.lock().unwrap().keys().cloned().collect::<Vec<PeerId>>();
                            peer_ids.sort();
                            let mut index = 99999;
                            let mut pks = vec![];
                            for (i, p) in peer_ids.iter().enumerate() {
                                let pk = *PKS.lock().unwrap().get(p).unwrap();
                                pks.push(pk);
                                if p.as_ref() == self.options.peer_id.as_ref() {
                                    index = i;
                                }
                            }
                            let party1_received_msg_round_1 = ROUND1
                                .lock()
                                .unwrap()
                                .clone()
                                .into_iter()
                                .filter(|(k, _)| k.as_ref() != self.options.peer_id.as_ref())
                                .map(|(_, v)| v)
                                .collect::<Vec<Vec<GE>>>();
                            let (c_party_1, r_party_1, _) = party_1_state.compute_global_params(
                                &msg,
                                &pks,
                                party1_received_msg_round_1,
                                index,
                            );

                            R.lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), r_party_1);
                            S.lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), s_total_1);
                            C.lock()
                                .unwrap()
                                .insert(self.options.topic.clone(), c_party_1);
                            println!("sign finish, R:{:?}, S:{:?}", r_party_1, s_total_1);
                        }
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
