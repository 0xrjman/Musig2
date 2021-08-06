use super::traits::push::Push;
use super::broadcast::BroadcastMsgs;
use super::Store;
use super::traits::state_machine::Msg;
use super::broadcast::BroadcastMsgsStore;
use crate::cli::protocals::musig2::{verify, GE, FE, KeyPair, KeyAgg, sign, sign_double_prime};
use crate::cli::protocals::{State, StatePrime};
use curv::BigInt;
use serde::{Deserialize, Serialize};
use curv::elliptic::curves::traits::ECPoint;

#[derive(Debug)]
pub struct Prepare {
    pub my_ind: u16,
    pub key_pair: KeyPair,
    pub message: Vec<u8>,
}

impl Prepare {
    pub fn proceed<O>(self, mut output: O) -> Result<Round1>
        where
            O: Push<Msg<MessageRound1>>,
    {
        let (ephemeral_keys, state1) = sign(self.key_pair.clone());

        output.push(
            Msg {
                sender: self.my_ind,
                receiver: None,
                body: MessageRound1 {
                    ephemeral_keys,
                    message: self.message.clone(),
                    pubkey: self.key_pair.clone().public_key,
                },
            }
        );

        Ok(Round1 {
            my_ind: self.my_ind,
            state1,
            key_pair: self.key_pair.clone(),
            message: self.message.clone(),
        })
    }
    pub fn is_expensive(&self) -> bool {
        // We assume that computing hash is expensive operation (in real-world, it's not)
        false
    }
}

#[derive(Debug)]
pub struct Round1 {
    pub my_ind: u16,
    pub state1: State,
    pub key_pair: KeyPair,
    pub message: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageRound1 {
    pub ephemeral_keys: Vec<GE>,
    pub message: Vec<u8>,
    pub pubkey: GE,
}


impl Round1 {
    pub fn proceed<O>(self, input: BroadcastMsgs<MessageRound1>, mut output: O) -> Result<Round2>
        where
            O: Push<Msg<MessageRound2>>,
    {
        let mut pks = vec![];
        let mut received_nonce = vec![];
        let cur_ind: usize = self.my_ind.into();

        for i in 0..input.msgs.len() {
            if i + 1 == cur_ind {
                pks.push(self.key_pair.public_key.clone());
            }
            pks.push(input.msgs[i].pubkey.clone());
            received_nonce.push(input.msgs[i].ephemeral_keys.clone());
        }
        if input.msgs.len() + 1 == cur_ind {
            pks.push(self.key_pair.public_key.clone());
        }
        let party_index: usize = (self.my_ind - 1) as usize;
        println!("pks:{:?}", pks);
        let key_agg = KeyAgg::key_aggregation_n(&pks, party_index);
        let (state2, sign_fragment) = self.state1.sign_prime(&self.message, &pks, received_nonce.clone(), party_index);
        let (commit, r, _) = self.state1.compute_global_params(&self.message, &pks, received_nonce, party_index);
        output.push(Msg {
            sender: self.my_ind,
            receiver: None,
            body: MessageRound2 {
                sign_fragment
            },
        });

        Ok(Round2 {
            my_ind: self.my_ind,
            commit,
            r,
            state2,
            key_pair: self.key_pair.clone(),
            key_agg,
        })
    }
    pub fn expects_messages(party_i: u16, party_n: u16) -> Store<BroadcastMsgs<MessageRound1>> {
        BroadcastMsgsStore::new(party_i, party_n)
    }

    pub fn is_expensive(&self) -> bool {
        // Sending cached message is the cheapest operation
        false
    }
}

#[derive(Debug)]
pub struct Round2 {
    pub my_ind: u16,
    pub commit: BigInt,
    pub r: GE,
    pub state2: StatePrime,
    pub key_pair: KeyPair,
    pub key_agg: KeyAgg,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageRound2 {
    pub sign_fragment: FE,
}

impl Round2 {
    pub fn proceed(self, input: BroadcastMsgs<MessageRound2>) -> Result<SignResult> {
        let mut received_round2 = vec![];
        for i in 0..input.msgs.len() {
            received_round2.push(input.msgs[i].sign_fragment.clone());
        }
        let s = sign_double_prime(self.state2, &received_round2);

        assert!(verify(&s, &self.r.x_coor().unwrap(), &self.key_agg.X_tilde, &self.commit).is_ok());
        println!("party index:{} verify success.", self.my_ind);
        Ok(SignResult {
            r: self.r.clone(),
            s,
            commit: self.commit.clone(),
        })
    }
    pub fn expects_messages(party_i: u16, party_n: u16) -> Store<BroadcastMsgs<MessageRound2>> {
        BroadcastMsgsStore::new(party_i, party_n)
    }
    pub fn is_expensive(&self) -> bool {
        // Round involves computing a hash, we assume it's expensive (again, in real-world it's not)
        false
    }
}

#[derive(Debug)]
pub struct SignResult {
    pub r: GE,
    pub s: FE,
    pub commit: BigInt,
}

// Messages

#[derive(Clone, Debug)]
pub struct CommittedSeed([u8; 32]);

#[derive(Clone, Debug)]
pub struct RevealedSeed {
    seed: u32,
    blinding: [u8; 32],
}

// Errors

type Result<T> = std::result::Result<T, ProceedError>;

#[derive(Debug, PartialEq)]
pub enum ProceedError {
    PartiesDidntRevealItsSeed { party_ind: Vec<u16> },
}

