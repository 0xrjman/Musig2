use super::{
    p2p::*,
    party::{Musig2Instance, Musig2Party},
};
use crate::cli::party::{
    async_protocol::AsyncProtocol,
    instance::ProtocolMessage,
    musig2::{incoming, Outgoing},
    traits::state_machine::{Msg, StateMachine},
    watcher::StderrWatcher,
    AsyncSession, MSESSION_ID,
};
use libp2p::{
    core::ConnectedPoint,
    futures::StreamExt,
    swarm::{Swarm, SwarmEvent},
    Multiaddr, PeerId,
};
use log::{debug, error, info};
use std::error::Error;
use tokio::{io::AsyncBufReadExt, sync::broadcast};

pub struct Node {
    swarm: TSwarm,
    pub party: Musig2Party<Multiaddr>,
    /// Responsible for receiving message from party
    pub rx_party: broadcast::Receiver<Msg<ProtocolMessage>>,
    /// Responsible for receiving message delivered internally
    pub rx_inter: broadcast::Receiver<CallMessage>,
}

impl Node {
    pub async fn init() -> Node {
        // Sending message from node to party
        let (tx_party, rx_node) = broadcast::channel(50);
        // Sending message from party to node
        let (tx_node, rx_party) = broadcast::channel(50);
        // Used as internal communication
        let (tx_inter, rx_inter) = broadcast::channel(50);

        let options = SwarmOptions::new_test_options(tx_inter, tx_party);
        let swarm = create_swarm(options.clone()).await.unwrap();

        let party = Musig2Party::new(tx_node, rx_node);

        Node {
            swarm,
            party,
            rx_party,
            rx_inter,
        }
    }

    pub async fn run_instance(&mut self, msg: Vec<u8>) {
        info!("try to generate instance, msg is {:?}", msg);

        let key_pair = self.swarm.behaviour_mut().options().keyring.clone();
        let cur_peer_id = self.swarm.behaviour_mut().options().peer_id;
        let mut peer_ids = self.party.peer_ids.clone();
        peer_ids.push(cur_peer_id);
        peer_ids.sort();
        let party_n = peer_ids.len() as u16;
        let mut party_i = 0;
        for (k, peer_id) in peer_ids.iter().enumerate() {
            if peer_id.as_ref() == cur_peer_id.as_ref() {
                party_i = (k + 1) as u16;
            }
        }
        info!("peer_ids: {:?}", peer_ids);
        info!("party_i: {:?}", party_i);
        assert!(party_i > 0, "party_i must be positive!");

        let instance = Musig2Instance::with_fixed_seed(party_i, party_n, msg, key_pair);
        let rx_node = self.swarm.behaviour_mut().options().tx_party.subscribe();
        // self.party.add_instance(instance, rx.clone());

        let parties = self.party.parties.clone();
        let peer_ids = self.party.peer_ids.clone();
        let tx_node = self.party.tx_node.clone();

        tokio::spawn(async move {
            // Receive messages from behaviour
            let incoming = incoming(rx_node, instance.party_ind());
            // Send message to behaviour
            let outgoing = Outgoing { sender: tx_node };
            // Create a async instance which can run as musig2
            let async_instance =
                AsyncProtocol::new(instance, incoming, outgoing).set_watcher(StderrWatcher);

            // Create a session that can execute musig2
            let mut msession = AsyncSession::with_fixed_instance(
                MSESSION_ID.to_string(),
                async_instance,
                parties,
                peer_ids,
            );

            info!("================ session.run start ==================");
            msession.run().await;
            info!("================ session.run over  ==================");
        });
    }

    pub fn publish_msg(&mut self, msg: Msg<ProtocolMessage>) {
        let topic = self.swarm.behaviour_mut().options().topic.clone();
        let json = serde_json::to_string(&msg).expect("can jsonify response");

        self.swarm
            .behaviour_mut()
            .floodsub
            .publish(topic, json.as_bytes());
    }

    pub fn call_peers(&mut self, msg: SignInfo) {
        let topic = self.swarm.behaviour_mut().options().topic.clone();
        let call = CallMessage::CoopSign(msg);
        let json = serde_json::to_string(&call).expect("can jsonify response");

        self.swarm
            .behaviour_mut()
            .floodsub
            .publish(topic, json.as_bytes());
    }

    pub fn add_party(&mut self, addr: Multiaddr, peer_id: PeerId) {
        self.party.add_party(addr);
        self.party.add_peer_id(peer_id);
    }

    pub fn remove_party(&mut self, addr: Multiaddr, peer_id: PeerId) {
        self.party.remove_party(addr);
        self.party.remove_peer_id(peer_id);
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
        let rest = cmd.strip_prefix("sign ").unwrap();
        let msg = Vec::from(rest.as_bytes());
        info!("handle sign {:?}", msg);

        self.run_instance(msg.clone()).await;
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let addr = self.swarm.behaviour_mut().options().listening_addrs.clone();
        Swarm::listen_on(&mut self.swarm, addr).expect("swarm can be started");

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        loop {
            let evt = {
                tokio::select! {
                    line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                    event = self.swarm.select_next_some() => {
                        match event {
                            SwarmEvent::ConnectionEstablished { endpoint, peer_id, .. } => {
                                if let ConnectedPoint::Dialer { address } = endpoint {
                                    self.add_party(address.clone(), peer_id);
                                    debug!("Connected in {:?}", address);
                                };
                            },
                            SwarmEvent::ConnectionClosed { endpoint, peer_id, .. } => {
                                if let ConnectedPoint::Dialer { address } = endpoint {
                                    self.remove_party(address.clone(), peer_id);
                                    debug!("Connection closed in {:?}", address);
                                };
                            },
                            _ => {
                                debug!("Unhandled Swarm Event: {:?}", event)
                            }
                        };
                        None
                    },
                    recv_party = self.rx_party.recv() => Some(EventType::AsyncResponse(recv_party.expect("recv exists"))),
                    recv_inter = self.rx_inter.recv() => Some(EventType::CallPeers(recv_inter.expect("recv_inter exists"))),
                }
            };

            if let Some(event) = evt {
                match event {
                    EventType::AsyncResponse(m) => {
                        self.publish_msg(m);
                        info!("publish msg succeed");
                    },
                    EventType::Response(_resp) => {
                        debug!("EventType::Response, has been deprecated")
                    },
                    EventType::CallPeers(call) => match call {
                        CallMessage::CoopSign(mut sign_info) => {
                            let cmd = sign_info.get_cmd("sign");
                            self.handle_sign(&cmd).await;
                        }
                    },
                    EventType::Input(line) => match line.as_str() {
                        cmd if cmd.starts_with("sign") => {
                            let msg = cmd.strip_prefix("sign ").unwrap();
                            info!("sign msg: {:?}", msg);
                            self.call_peers(SignInfo::new(msg.to_string()));
                            self.handle_sign(cmd).await;
                        }
                        cmd if cmd.starts_with("verify") => {
                            // todo! Verify the signature
                        }
                        // List the peer nodes that are currently connected
                        // Example: $ list p
                        cmd if cmd.starts_with("list") => self.handle_list(cmd).await,
                        _ => error!("unknown command"),
                    },
                    EventType::Send(m) => match m {
                        Message::Round1(_r1) => {
                            debug!("send round1, has been deprecated");
                        }
                        Message::Round2(_r2) => {
                            debug!("send round2, has been deprecated");
                        }
                    },
                }
            }
        }
    }
}
