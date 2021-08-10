use super::{
    p2p::*,
    party::{Musig2Instance, Musig2Party},
};
use crate::*;
use libp2p::{
    core::ConnectedPoint,
    floodsub::Topic,
    futures::StreamExt,
    swarm::{Swarm, SwarmEvent},
    Multiaddr,
};
use log::{debug, error, info};
use std::error::Error;
use tokio::io::AsyncBufReadExt;

pub struct Node {
    swarm: TSwarm,
    pub party: Musig2Party<Multiaddr>,
    pub other: Other,
}

pub struct Other {
    topic: Topic,
}

impl Node {
    pub async fn init() -> Node {
        let options = SwarmOptions::new_test_options();
        let mut swarm = create_swarm(options.clone()).await.unwrap();
        let topic = swarm.behaviour_mut().options().topic.clone();

        let instance = Musig2Instance::with_fixed_seed(1, 3, Vec::from("test"), options.keyring);

        let party = Musig2Party::new(instance);

        Node {
            swarm,
            party,
            other: Other { topic },
        }
    }

    pub fn add_party(&mut self, endpoint: ConnectedPoint) {
        self.party.add_party(connection_point_addr(endpoint));
    }

    pub fn remove_party(&mut self, endpoint: ConnectedPoint) {
        self.party.remove_party(connection_point_addr(endpoint));
    }

    pub fn list_parties(&mut self) {
        // let peer_id = self.swarm.behaviour_mut().options().peer_id;
        info!("Connected parties:");
        for p in self.party.parties.iter() {
            info!("{}", p);
        }
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
                            SwarmEvent::ConnectionEstablished { endpoint, .. } => {
                                self.add_party(endpoint.clone());
                                self.list_parties();
                                info!("Connected in {:?}", is_dialer_connection(endpoint));
                            },
                            SwarmEvent::ConnectionClosed { endpoint, .. } => {
                                self.remove_party(endpoint.clone());
                                self.list_parties();
                                debug!("Connection closed in {:?}", connection_point_addr(endpoint));
                            },
                            _ => {
                                debug!("Unhandled Swarm Event: {:?}", event)
                            }
                        };
                        None
                    },
                    // response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
                    send = state_into_event(SIGNSTATE.lock().unwrap().get(&self.other.topic.clone()).unwrap().clone()) => send,
                }
            };

            if let Some(event) = evt {
                match event {
                    EventType::Response(resp) => {
                        let json = serde_json::to_string(&resp).expect("can jsonify response");
                        self.swarm
                            .behaviour_mut()
                            .floodsub()
                            .publish(TOPIC.to_owned(), json.as_bytes());
                    }
                    EventType::Input(line) => match line.as_str() {
                        cmd if cmd.starts_with("sign") => handle_sign(cmd, &mut self.swarm).await,
                        cmd if cmd.starts_with("verify") => {
                            self.swarm.behaviour_mut().verify_sign().await
                        }
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

pub async fn handle_sign(cmd: &str, swarm: &mut Swarm<SignatureBehaviour>) {
    let topic = swarm.behaviour_mut().options().topic.clone();
    let rest = cmd.strip_prefix("sign ").unwrap();
    let msg = Vec::from(rest.as_bytes());
    let state = SIGNSTATE.lock().unwrap().get(&topic).unwrap().clone();
    if let SignState::Round2End = state {
        SIGNSTATE
            .lock()
            .unwrap()
            .insert(topic.clone(), SignState::Round1Send);
        println!(
            "peerid start sign, send round1:{:?}",
            swarm.behaviour_mut().options().peer_id.clone()
        );
        MSG.lock().unwrap().insert(topic, msg.clone());
        swarm.behaviour_mut().solve_msg_in_generating_round_1(msg);
    };
}
