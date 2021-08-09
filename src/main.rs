use libp2p::{floodsub::Topic, PeerId};
use once_cell::sync::Lazy;

use curv::elliptic::curves::secp256_k1::{FE, GE};
use curv::BigInt;
use std::collections::HashMap;
use std::sync::Mutex;

use lazy_static::lazy_static;

static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("test"));
static KEY_PAIR: Lazy<KeyPair> = Lazy::new(KeyPair::create);

lazy_static! {
    static ref PKS: Mutex<HashMap<PeerId, GE>> = Mutex::new(HashMap::<PeerId, GE>::new());
    static ref ROUND1: Mutex<HashMap<PeerId, Vec<GE>>> =
        Mutex::new(HashMap::<PeerId, Vec<GE>>::new());
    static ref ROUND2: Mutex<HashMap<PeerId, FE>> = Mutex::new(HashMap::<PeerId, FE>::new());
    static ref SIGNSTATE: Mutex<HashMap<Topic, SignState>> =
        Mutex::new(HashMap::<Topic, SignState>::new());
    static ref PENDINGSTATE: Mutex<HashMap<Topic, SignState>> =
        Mutex::new(HashMap::<Topic, SignState>::new());
    static ref STATE0: Mutex<HashMap<Topic, State>> = Mutex::new(HashMap::<Topic, State>::new());
    static ref STATE1: Mutex<HashMap<Topic, StatePrime>> =
        Mutex::new(HashMap::<Topic, StatePrime>::new());
    static ref KEYAGG: Mutex<HashMap<Topic, KeyAgg>> = Mutex::new(HashMap::<Topic, KeyAgg>::new());
    static ref MSG: Mutex<HashMap<Topic, Vec<u8>>> = Mutex::new(HashMap::<Topic, Vec<u8>>::new());
    static ref R: Mutex<HashMap<Topic, GE>> = Mutex::new(HashMap::<Topic, GE>::new());
    static ref S: Mutex<HashMap<Topic, FE>> = Mutex::new(HashMap::<Topic, FE>::new());
    static ref C: Mutex<HashMap<Topic, BigInt>> = Mutex::new(HashMap::<Topic, BigInt>::new());
}

mod cli;
use cli::node::Node;
use cli::p2p::behaviour::*;
use cli::protocals::signature::*;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let mut node = Node::init().await;
    node.run().await.unwrap();
}
