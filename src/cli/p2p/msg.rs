use serde::{Deserialize, Serialize};

use crate::cli::{
    party::{instance::ProtocolMessage, traits::state_machine::Msg},
    protocals::signature::*,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum EventType {
    Response(Message),
    Input(String),
    Send(Message),
    AsyncResponse(Msg<ProtocolMessage>),
    CallPeers(CallMessage),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Message {
    Round1(Round1),
    Round2(Round2),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CallMessage {
    CoopSign(SignInfo),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SignInfo {
    pub msg: String,
}

impl SignInfo {
    pub fn new(msg: String) -> Self {
        Self { msg }
    }
    pub fn get_cmd(&mut self, cmd: &str) -> String {
        let order = format!("{} {}", cmd, self.msg);
        order
    }
}
