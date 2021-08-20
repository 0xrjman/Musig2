use super::{
    broadcast::{BroadcastMsgs, BroadcastMsgsStore},
    traits::{push::Push, state_machine::Msg},
    Store,
};
use crate::cli::protocals::{
    error::Musig2Error,
    key::{PrivateKey, PublicKey},
    musig2::*,
    signature::*,
};
use log::warn;
use secp256k1::Message;
use serde::{Deserialize, Serialize};

/// Prepare round performs preprocessing operations to construct messages for the `Round1` of communication.
///
/// The main work of the preparation process is to generate nonce and construct messages.
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
        // Generate `nonce` from the held private key
        let (nonce, state1) = sign(self.key_pair.clone())?;

        // The message of the `Round1` needs to pass `nonce` and `public key`
        //
        // Nonce is necessary for the musig2 scheme, but due to the adoption of libp2p
        // it is difficult for participants to exchange the public key offline in advance
        // so the public key is also exchanged in the `Round1` of messages.
        output.push(Msg {
            sender: self.my_ind,
            receiver: None,
            body: MessageRound1 {
                ephemeral_keys: PublicKey::convert_to_vec(nonce),
                message: self.message.clone(),
                pubkey: self.key_pair.public_key.serialize().to_vec(),
            },
        });

        Ok(Round1 {
            my_ind: self.my_ind,
            state1,
            key_pair: self.key_pair.clone(),
            message: self.message,
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
    pub ephemeral_keys: Vec<Vec<u8>>,
    pub message: Vec<u8>,
    pub pubkey: Vec<u8>,
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
            let mut tt = [0u8; 65];
            tt.copy_from_slice(input.msgs[i].pubkey.as_slice());

            if let Ok(pk) = PublicKey::parse(&tt) {
                pks.push(pk);
            } else {
                warn!("pks push failed, `PublicKey::parse(&tt)` meet error");
            }
            received_nonce.push(PublicKey::convert_from_vec(
                input.msgs[i].ephemeral_keys.clone(),
            ));
        }
        if input.msgs.len() + 1 == cur_ind {
            pks.push(self.key_pair.public_key.clone());
        }
        let party_index: usize = (self.my_ind - 1) as usize;
        println!("pks:{:?}", pks);
        let key_agg = KeyAgg::key_aggregation_n(&pks, party_index)?;
        let (state2, sign_fragment) =
            self.state1
                .sign_prime(&self.message, &pks, received_nonce.clone(), party_index)?;
        let (commit, r, _) =
            self.state1
                .compute_global_params(&self.message, &pks, received_nonce, party_index)?;
        output.push(Msg {
            sender: self.my_ind,
            receiver: None,
            body: MessageRound2 {
                sign_fragment: sign_fragment.serialize().to_vec(),
            },
        });

        Ok(Round2 {
            my_ind: self.my_ind,
            commit,
            r,
            state2,
            key_pair: self.key_pair,
            key_agg,
            message: self.message,
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
    pub commit: PrivateKey,
    pub r: PublicKey,
    pub state2: StatePrime,
    pub key_pair: KeyPair,
    pub key_agg: KeyAgg,
    pub message: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageRound2 {
    pub sign_fragment: Vec<u8>,
}

impl Round2 {
    pub fn proceed(self, input: BroadcastMsgs<MessageRound2>) -> Result<SignResult> {
        let mut received_round2 = vec![];
        for i in 0..input.msgs.len() {
            received_round2.push(PrivateKey::parse_slice(&input.msgs[i].sign_fragment)?);
        }
        let s = sign_double_prime(self.state2, &received_round2)?;

        let signature = Signature {
            rx: PrivateKey::parse_slice(&self.r.x_coor()).unwrap(),
            s: s.clone(),
        };

        assert!(verify(
            &signature,
            &Message::parse_slice(&self.message).unwrap(),
            &self.key_agg.X_tilde,
        )
        .is_ok());

        println!("party index:{} verify success.", self.my_ind);
        Ok(SignResult {
            r: self.r,
            s,
            commit: self.commit,
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
    pub r: PublicKey,
    pub s: PrivateKey,
    pub commit: PrivateKey,
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
    Musig2Error,
}

impl From<Musig2Error> for ProceedError {
    fn from(_: Musig2Error) -> Self {
        ProceedError::Musig2Error
    }
}
