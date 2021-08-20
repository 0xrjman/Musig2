use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::{fmt, mem::replace, time::Duration};

use super::{
    broadcast::BroadcastMsgs,
    rounds,
    rounds::{MessageRound1, MessageRound2, Prepare, ProceedError, Round1, Round2, SignResult},
    store_err::StoreErr,
    traits::push::{Push, PushExt},
    traits::{
        message::MessageStore,
        state_machine::{IsCritical, Msg, StateMachine},
    },
    Store,
};
use crate::cli::protocals::musig2::KeyPair;

pub struct Musig2Instance {
    round: R,
    msgs1: Option<Store<BroadcastMsgs<MessageRound1>>>,
    msgs2: Option<Store<BroadcastMsgs<MessageRound2>>>,
    msgs_queue: Vec<Msg<ProtocolMessage>>,
    party_i: u16,
    party_n: u16,
}

impl Musig2Instance {
    pub fn with_fixed_seed(
        party_i: u16,
        party_n: u16,
        message: Vec<u8>,
        key_pair: KeyPair,
    ) -> Self {
        Self {
            party_i,
            party_n,
            round: R::Prepare(Prepare {
                my_ind: party_i,
                key_pair,
                message,
            }),
            msgs1: Some(Round1::expects_messages(party_i, party_n)),
            msgs2: Some(Round2::expects_messages(party_i, party_n)),
            msgs_queue: vec![],
        }
    }

    fn gmap_queue<'a, T, F>(&'a mut self, mut f: F) -> impl Push<Msg<T>> + 'a
    where
        F: FnMut(T) -> M + 'a,
    {
        (&mut self.msgs_queue).gmap(move |m: Msg<T>| m.map_body(|m| ProtocolMessage(f(m))))
    }

    /// Proceeds round state if it received enough messages and if it's cheap to compute or
    /// `may_block == true`
    fn proceed_round(&mut self, may_block: bool) -> Result<()> {
        // Check whether enough messages have been received to complete the `Round1` of musig2
        let store1_wants_more = self.msgs1.as_ref().map(|s| s.wants_more()).unwrap_or(false);
        // Check whether enough messages have been received to complete the `Round2` of musig2
        let store2_wants_more = self.msgs2.as_ref().map(|s| s.wants_more()).unwrap_or(false);

        let next_state: R;
        let try_again: bool = match replace(&mut self.round, R::Gone) {
            // Proceed the `Prepare` round
            // which will construct the `Round1` message and add it to the corresponding message queue
            R::Prepare(p) if !p.is_expensive() || may_block => {
                info!("R::Prepare {:?}", p);
                // After proceed `Prepare` round, next_state is `Round1`
                next_state = p
                    .proceed(self.gmap_queue(M::Round1))
                    .map(R::Round1)
                    .map_err(Error::ProceedRound)?;

                true
            }
            // `Prepare` round do not need to wait for other events (e.g. message queue has enough messages)
            // The calculation is also cheap and thus always executable, so that this branch is generally not executed
            s @ R::Prepare(_) => {
                info!("R::Prepare next");
                next_state = s;
                false
            }
            // Proceed the `Round1` round if enough messages are received,
            // which will construct the `Round2` message and add it to the corresponding message queue
            R::Round1(round) if !store1_wants_more && (!round.is_expensive() || may_block) => {
                info!("R::Round1 {:?}", round);
                let store = self.msgs1.take().expect("store gone before round complete");
                let msgs = store.finish().map_err(Error::HandleMsg)?;
                // After proceed `Round1` round, next_state is `Round2`
                next_state = round
                    .proceed(msgs, self.gmap_queue(M::Round2))
                    .map(R::Round2)
                    .map_err(Error::ProceedRound)?;
                true
            }
            // Stay in Round1
            // Have not received enough messages, do not process them now
            s @ R::Round1(_) => {
                info!("R::Round1 next");
                next_state = s;
                false
            }
            // Proceed the `Round2` round if enough messages are received,
            // After processing, the result of the musig2 aggregate signature is generated
            R::Round2(round) if !store2_wants_more && (!round.is_expensive() || may_block) => {
                info!("R::Round2 {:?}", round);
                let store = self.msgs2.take().expect("store gone before round complete");
                let msgs = store.finish().map_err(Error::HandleMsg)?;
                //After `Round2` is processed, it enters the `Finish` round
                next_state = round
                    .proceed(msgs)
                    .map(R::Finished)
                    .map_err(Error::ProceedRound)?;
                false
            }
            // Stay in Round2
            // Have not received enough messages, do not process them now
            s @ R::Round2(_) => {
                info!("R::Round2 next, waiting for messages",);
                next_state = s;
                false
            }
            s @ R::Finished(_) | s @ R::Gone => {
                info!("R::Finished");
                next_state = s;
                false
            }
        };

        self.round = next_state;
        if try_again {
            self.proceed_round(may_block)
        } else {
            Ok(())
        }
    }
}

impl StateMachine for Musig2Instance {
    type MessageBody = ProtocolMessage;
    type Err = Error;
    type Output = SignResult;

    // Proceed incoming messages
    fn handle_incoming(&mut self, msg: Msg<Self::MessageBody>) -> Result<()> {
        let current_round = self.current_round();
        info!("msg sender is {:?}", msg.sender);
        match msg.body {
            ProtocolMessage(M::Round1(m)) => {
                // `[critical-error]` Check whether the received message is out of date
                let store = self.msgs1.as_mut().ok_or(Error::OutOfOrderMsg {
                    current_round,
                    msg_round: 1,
                })?;
                // `[non-critical-error]` Check whether the received message can pass the pre-validation
                store
                    .push_msg(Msg {
                        sender: msg.sender,
                        receiver: msg.receiver,
                        body: m,
                    })
                    .map_err(Error::HandleMsg)?;
                self.proceed_round(false)
            }
            ProtocolMessage(M::Round2(m)) => {
                let store = self.msgs2.as_mut().ok_or(Error::OutOfOrderMsg {
                    current_round,
                    msg_round: 1,
                })?;
                store
                    .push_msg(Msg {
                        sender: msg.sender,
                        receiver: msg.receiver,
                        body: m,
                    })
                    .map_err(Error::HandleMsg)?;
                self.proceed_round(false)
            }
        }
    }

    fn message_queue(&mut self) -> &mut Vec<Msg<Self::MessageBody>> {
        &mut self.msgs_queue
    }

    fn wants_to_proceed(&self) -> bool {
        let store1_wants_more = self.msgs1.as_ref().map(|s| s.wants_more()).unwrap_or(false);
        let store2_wants_more = self.msgs2.as_ref().map(|s| s.wants_more()).unwrap_or(false);

        match self.round {
            // `Prepare` round always need to be performed
            R::Prepare(_) => true,
            // Proceed the `Round1` when there are enough messages.
            R::Round1(_) => !store1_wants_more,
            // Proceed the `Round2` when there are enough messages.
            R::Round2(_) => !store2_wants_more,
            // If it is finished, it will not be proceed any further.
            R::Finished(_) | R::Gone => false,
        }
    }

    fn proceed(&mut self) -> Result<()> {
        self.proceed_round(true)
    }

    fn round_timeout(&self) -> Option<Duration> {
        if matches!(self.round, R::Round2(_)) {
            Some(Duration::from_secs(5))
        } else {
            None
        }
    }

    fn round_timeout_reached(&mut self) -> Self::Err {
        if !matches!(self.round, R::Round2(_)) {
            panic!("no timeout was set")
        }
        let (_, parties) = self
            .msgs2
            .as_ref()
            .expect("store is gone, but round is not over yet")
            .blame();
        Error::ProceedRound(ProceedError::PartiesDidntRevealItsSeed { party_ind: parties })
    }

    fn is_finished(&self) -> bool {
        matches!(self.round, R::Finished(_))
    }

    fn pick_output(&mut self) -> Option<Result<Self::Output>> {
        match self.round {
            R::Finished(_) => (),
            R::Gone => return Some(Err(Error::DoublePickResult)),
            _ => return None,
        }

        match replace(&mut self.round, R::Gone) {
            R::Finished(result) => Some(Ok(result)),
            _ => unreachable!("guaranteed by match expression above"),
        }
    }

    fn current_round(&self) -> u16 {
        match self.round {
            R::Prepare(_) => 0,
            R::Round1(_) => 1,
            R::Round2(_) => 2,
            R::Finished(_) | R::Gone => 3,
        }
    }

    fn total_rounds(&self) -> Option<u16> {
        Some(2)
    }

    fn party_ind(&self) -> u16 {
        self.party_i
    }

    fn parties(&self) -> u16 {
        self.party_n
    }
}

impl fmt::Debug for Musig2Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let current_round = match &self.round {
            R::Prepare(_) => "0",
            R::Round1(_) => "1",
            R::Round2(_) => "2",
            R::Finished(_) => "[Finished]",
            R::Gone => "[Gone]",
        };
        let msgs1 = match self.msgs1.as_ref() {
            Some(msgs) => format!("[{}/{}]", msgs.messages_received(), msgs.messages_total()),
            None => "[None]".into(),
        };
        let msgs2 = match self.msgs2.as_ref() {
            Some(msgs) => format!("[{}/{}]", msgs.messages_received(), msgs.messages_total()),
            None => "[None]".into(),
        };
        write!(
            f,
            "{{MPCRandom at round={} msgs1={} msgs2={} queue=[len={}]}}",
            current_round,
            msgs1,
            msgs2,
            self.msgs_queue.len()
        )
    }
}

// Rounds
#[allow(clippy::large_enum_variant)]
pub enum R {
    Prepare(Prepare),
    Round1(Round1),
    Round2(Round2),
    Finished(SignResult),
    Gone,
}

// Messages

/// Protocol message
///
/// Hides message structure so it could be changed without breaking semver policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtocolMessage(M);

#[derive(Clone, Debug, Serialize, Deserialize)]
enum M {
    Round1(rounds::MessageRound1),
    Round2(rounds::MessageRound2),
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// Protocol error caught at proceeding round
    ProceedRound(ProceedError),
    /// Received message didn't pass pre-validation
    HandleMsg(StoreErr),
    /// Received message which we didn't expect to receive (e.g. message from previous round)
    OutOfOrderMsg { current_round: u16, msg_round: u16 },
    /// [MusigInstance::pick_output] called twice
    DoublePickResult,
}

impl IsCritical for Error {
    fn is_critical(&self) -> bool {
        // Protocol is not resistant to occurring any of errors :(
        match self {
            Error::ProceedRound(_) => {
                warn!("Error::ProceedRound, critical error");
                true
            }
            Error::HandleMsg { .. } => {
                warn!("Error::HandleMsg, non-critical error");
                false
            }
            Error::OutOfOrderMsg { .. } => {
                warn!("Error::OutOfOrderMsg, critical error");
                true
            }
            Error::DoublePickResult => {
                warn!("Error::DoublePickResult, critical error");
                true
            }
        }
    }
}
