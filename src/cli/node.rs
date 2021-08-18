use super::{
    p2p::*,
    party::{Musig2Instance, Musig2Party},
};
use crate::cli::party::async_protocol::AsyncProtocol;
use crate::cli::party::musig2::{incoming, Outgoing};
use crate::cli::party::traits::state_machine::StateMachine;
use crate::cli::party::{async_protocol, watcher::StderrWatcher};
use crate::cli::party::{instance::ProtocolMessage, traits::state_machine::Msg};
use crate::*;
use crate::cli::party::{MSESSION_ID, MSession};
use libp2p::{
    core::ConnectedPoint,
    floodsub::Topic,
    futures::StreamExt,
    swarm::{Swarm, SwarmEvent},
    Multiaddr, PeerId,
};
use log::{debug, error, info};
use std::cell::RefCell;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;
use tokio::sync::broadcast;

pub struct Node {
    swarm: TSwarm,
    pub party: Musig2Party<Multiaddr>,
    pub other: Other,
    /// Responsible for sending the data of the state machine to other parties
    pub rx: broadcast::Receiver<Msg<ProtocolMessage>>,
}

pub struct Other {
    topic: Topic,
}

impl Node {
    pub async fn init() -> Node {
        let (tx1, _) = broadcast::channel(50);
        let (tx2, rx2) = broadcast::channel(50);

        let options = SwarmOptions::new_test_options(tx1);
        let mut swarm = create_swarm(options.clone()).await.unwrap();
        let topic = swarm.behaviour_mut().options().topic.clone();

        let party = Musig2Party::new(tx2);

        Node {
            rx: rx2,
            swarm,
            party,
            other: Other { topic },
        }
    }

    pub async fn gen_instance(&mut self, msg: Vec<u8>) {
        info!("try to generate instance, msg is {:?}", msg);
        let key_pair = self.swarm.behaviour_mut().options().keyring.clone();
        let cur_peer_id = self.swarm.behaviour_mut().options().peer_id;
        let mut peer_ids = self.party.peer_ids.clone();
        peer_ids.push(cur_peer_id.clone());
        peer_ids.sort();
        let party_n = peer_ids.len() as u16;
        let mut party_i = 0;
        for (k, peer_id) in peer_ids.iter().enumerate() {
            if peer_id.as_ref() == cur_peer_id.as_ref() {
                party_i = (k + 1) as u16;
            }
        }
        assert!(party_i > 0, "party_i must be positive!");

        let instance = Musig2Instance::with_fixed_seed(party_i, party_n, msg, key_pair);
        let rx = self.swarm.behaviour_mut().options().tx.subscribe();
        // self.party.add_instance(instance, rx.clone());



        // let party = Arc::clone(&self.party.instance);

        // // todo!: Asynchronous thread runs up the state machine
        // let h = tokio::spawn(async move {
        //     if let Some(s) = party.lock().unwrap().as_mut() {
        //         s.run();
        //     };
        // });
        
        let parties = self.party.parties.clone();
        let peer_ids = self.party.peer_ids.clone();
        let tx = self.party.tx.clone();

        // let mut msession = 
        //     self.party.instances
        //         .get(MSESSION_ID).unwrap();
        let h = tokio::spawn(async move {
            // Receive messages from behaviour
            let incoming = incoming(rx, instance.party_ind());
            // Send message to behaviour
            let outgoing = Outgoing { sender: tx };
            // Create a async instance which can run as musig2
            let async_instance = AsyncProtocol::new(instance, incoming, outgoing).set_watcher(StderrWatcher);
            
            // Create a session that can execute musig2
            let mut msession = MSession::with_fixed_instance(
                MSESSION_ID.to_string(),
                async_instance,
                parties,
                peer_ids,
            );

            info!("================ msession.run start ==================");
            msession.run().await;
            info!("================ msession.run over  ==================");
        });
        // if let Some(msession) = _instance { 
        // }
    }

    pub fn add_party(&mut self, addr: Multiaddr, peer_id: PeerId) {
        self.party.add_party(addr);
        self.party.add_peer_id(peer_id);
    }

    /// Solve address of [ConnectedPoint::Listener] and [ConnectedPoint::Dialer]
    pub fn remove_party(&mut self, addr: Multiaddr) {
        self.party.remove_party(addr);
    }

    pub fn list_parties(&mut self) {
        // let peer_id = self.swarm.behaviour_mut().options().peer_id;
        info!("Connected parties:");
        for p in self.party.parties.iter() {
            info!("{}", p);
        }
    }

    async fn handle_list(&mut self, cmd: &str) {
        let order = cmd.strip_prefix("list ").unwrap();
        info!("handle list {:?}", order);
        self.list_parties();
    }

    pub async fn handle_sign(&mut self, cmd: &str) {
        // let topic = swarm.behaviour_mut().options().topic.clone();
        let rest = cmd.strip_prefix("sign ").unwrap();
        let msg = Vec::from(rest.as_bytes());
        // let state = SIGNSTATE.lock().unwrap().get(&topic).unwrap().clone();
        // if let SignState::Round2End = state {
        //     SIGNSTATE
        //         .lock()
        //         .unwrap()
        //         .insert(topic.clone(), SignState::Round1Send);
        //     println!(
        //         "peerid start sign, send round1:{:?}",
        //         swarm.behaviour_mut().options().peer_id.clone()
        //     );
        //     MSG.lock().unwrap().insert(topic, msg.clone());
        //     swarm.behaviour_mut().solve_msg_in_generating_round_1(msg);
        // };
        self.gen_instance(msg).await;
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let addr = self.swarm.behaviour_mut().options().listening_addrs.clone();
        Swarm::listen_on(&mut self.swarm, addr).expect("swarm can be started");

        SIGNSTATE
            .lock()
            .unwrap()
            .insert(TOPIC.to_owned(), SignState::Round2End);

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        loop {
            let evt = {
                tokio::select! {
                    line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                    event = self.swarm.select_next_some() => {
                        match event {
                            SwarmEvent::ConnectionEstablished { endpoint, peer_id, .. } => {
                                if let ConnectedPoint::Dialer { address } = endpoint {
                                    self.add_party(address.clone(), peer_id.clone());
                                    debug!("Connected in {:?}", address);
                                };
                            },
                            SwarmEvent::ConnectionClosed { endpoint, .. } => {
                                if let ConnectedPoint::Dialer { address } = endpoint {
                                    self.remove_party(address.clone());
                                    debug!("Connection closed in {:?}", address);
                                };
                            },
                            _ => {
                                debug!("Unhandled Swarm Event: {:?}", event)
                            }
                        };
                        None
                    },
                    // response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
                    send = state_into_event(SIGNSTATE.lock().unwrap().get(&self.other.topic.clone()).unwrap().clone()) => send,
                    recv = self.rx.recv() => Some(EventType::Response1(recv.expect("recv exists"))),
                }
            };

            if let Some(event) = evt {
                match event {
                    EventType::Response1(m) => {
                        // todo! Receive data from the outgoing tx of the state machine and publish through swam
                        println!("Response1:{:?}", m);
                    }
                    EventType::Response(resp) => {
                        let json = serde_json::to_string(&resp).expect("can jsonify response");
                        self.swarm
                            .behaviour_mut()
                            .floodsub()
                            .publish(TOPIC.to_owned(), json.as_bytes());
                    }
                    EventType::Input(line) => match line.as_str() {
                        cmd if cmd.starts_with("sign") => self.handle_sign(cmd).await,
                        cmd if cmd.starts_with("verify") => {
                            self.swarm.behaviour_mut().verify_sign().await
                        }
                        cmd if cmd.starts_with("list") => self.handle_list(cmd).await,
                        _ => error!("unknown command"),
                    },
                    EventType::Send(m) => {
                        self.swarm.behaviour_mut().floodsub().publish(
                            TOPIC.to_owned(),
                            serde_json::to_string(&m).unwrap().as_bytes(),
                        );
                        match m {
                            Message::Round1(r1) => {
                                let peer_id = self.swarm.behaviour_mut().options().peer_id;
                                println!("{:?} send round1", peer_id);
                                MSG.lock().unwrap().insert(TOPIC.to_owned(), r1.msg.clone());
                                if let Some(s) = PENDINGSTATE.lock().unwrap().get(&TOPIC.to_owned())
                                {
                                    SIGNSTATE
                                        .lock()
                                        .unwrap()
                                        .insert(TOPIC.to_owned(), s.clone());
                                } else {
                                    SIGNSTATE
                                        .lock()
                                        .unwrap()
                                        .insert(TOPIC.to_owned(), SignState::Round1Send);
                                }
                            }
                            Message::Round2(_) => {
                                let peer_id = self.swarm.behaviour_mut().options().peer_id;
                                println!("{:?} send round2", peer_id);
                                SIGNSTATE
                                    .lock()
                                    .unwrap()
                                    .insert(TOPIC.to_owned(), SignState::Round2Send);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn state_into_event(e: SignState) -> Option<EventType> {
    match e {
        SignState::Prepare(r1) => Some(EventType::Send(r1)),
        SignState::Round1End(r2) => Some(EventType::Send(r2)),
        SignState::Round1Send => {
            if let Some(s) = PENDINGSTATE.lock().unwrap().get(&TOPIC.to_owned()) {
                SIGNSTATE
                    .lock()
                    .unwrap()
                    .insert(TOPIC.to_owned(), s.clone());
                None
            } else {
                None
            }
        }
        _ => None,
    }
}

// pub async fn handle_sign(cmd: &str, swarm: &mut Swarm<SignatureBehaviour>) {
//     // let topic = swarm.behaviour_mut().options().topic.clone();
//     let rest = cmd.strip_prefix("sign ").unwrap();
//     let msg = Vec::from(rest.as_bytes());
//     // let state = SIGNSTATE.lock().unwrap().get(&topic).unwrap().clone();
//     // if let SignState::Round2End = state {
//     //     SIGNSTATE
//     //         .lock()
//     //         .unwrap()
//     //         .insert(topic.clone(), SignState::Round1Send);
//     //     println!(
//     //         "peerid start sign, send round1:{:?}",
//     //         swarm.behaviour_mut().options().peer_id.clone()
//     //     );
//     //     MSG.lock().unwrap().insert(topic, msg.clone());
//     //     swarm.behaviour_mut().solve_msg_in_generating_round_1(msg);
//     // };
//     // let
// }
