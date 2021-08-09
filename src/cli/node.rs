use libp2p::{
    floodsub::Topic,
    futures::StreamExt,
    swarm::{Swarm, SwarmEvent},
};
use log::{debug, error};
use std::error::Error;
use tokio::io::AsyncBufReadExt;

use super::p2p::*;
use crate::*;
pub struct Node {
    swarm: TSwarm,
    pub other: Other,
}

pub struct Other {
    topic: Topic,
}

impl Node {
    pub async fn init() -> Node {
        // let (response_sender, mut _response_rcv) = mpsc::unbounded_channel();
        let options = SwarmOptions::new_test_options();
        let mut swarm = create_swarm(options).await.unwrap();
        let topic = swarm.behaviour_mut().options().topic.clone();

        Node {
            swarm,
            other: Other { topic },
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let addr = self.swarm.behaviour_mut().options().listening_addrs.clone();
        Swarm::listen_on(&mut self.swarm, addr).expect("swarm can be started");

        SIGNSTATE
            .lock()
            .unwrap()
            .insert(TOPIC.to_owned(), SignState::Round2End);

        loop {
            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            let evt = {
                tokio::select! {
                    line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                    event = self.swarm.select_next_some() => {
                        match event {
                            SwarmEvent::ConnectionEstablished { endpoint, .. } => {
                                debug!("Connected in {:?}", endpoint)
                            },
                            SwarmEvent::ConnectionClosed { endpoint, .. } => {
                                debug!("Connection closed in {:?}", endpoint)
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
                                println!(
                                    "peerid send round1:{:?}",
                                    self.swarm.behaviour_mut().options().peer_id.clone()
                                );
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
                                println!(
                                    "peerid send round2:{:?}",
                                    self.swarm.behaviour_mut().options().peer_id.clone()
                                );
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
