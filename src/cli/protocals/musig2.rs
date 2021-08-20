/*
    Copyright 2020 by Kzen Networks
    This file is part of Multisig Schnorr library
    (https://github.com/KZen-networks/multisig-schnorr)
    Multisig Schnorr is free software: you can redistribute
    it and/or modify it under the terms of the GNU General Public
    License as published by the Free Software Foundation, either
    version 3 of the License, or (at your option) any later version.
    @license GPL-3.0+ <https://github.com/KZen-networks/multisig-schnorr/blob/master/LICENSE>
*/

//! Two-round Multisig Schnorr
//! From Jonas Nick, Tim Ruffing, and Yannick Seurin "MuSig2: Simple Two-Round Schnorr Multi-Signatures"
//! This implementation is based on https://eprint.iacr.org/2020/1261 page 12
//! The naming of the functions, states, and variables is aligned with that of the protocol
//! The number of shares Nv is set to 2 for which the authors claim to be secure assuming ROM and AGM

#![allow(non_snake_case)]

use super::error::{Musig2Error, Musig2Error::Invalid};
use core::ops::Neg;
use secp256k1::{
    curve::{Affine, Jacobian, Scalar, ECMULT_CONTEXT},
    Message,
};

use digest::Digest;
use light_bitcoin_schnorr::taggedhash::*;

use super::key::{PrivateKey, PublicKey};
use crate::cli::protocals::signature::Signature;

#[allow(non_upper_case_globals)]
const Nv: usize = 2;

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub public_key: PublicKey,
    private_key: PrivateKey,
}

impl KeyPair {
    pub fn create() -> Result<KeyPair, Musig2Error> {
        let private_key = PrivateKey::generate_random()?;
        let public_key = PublicKey::create_from_private_key(&private_key);

        Ok(KeyPair {
            public_key,
            private_key,
        })
    }

    #[allow(dead_code)]
    pub fn create_from_private_key(private_key: &[u8; 32]) -> Result<KeyPair, Musig2Error> {
        let private_key = PrivateKey::parse(private_key)?;
        let public_key = PublicKey::create_from_private_key(&private_key);
        Ok(KeyPair {
            public_key,
            private_key,
        })
    }
}

#[derive(Debug, Clone)]
pub struct KeyAgg {
    pub X_tilde: PublicKey,
    pub a_i: PrivateKey,
}

impl KeyAgg {
    pub fn key_aggregation_n(pks: &[PublicKey], party_index: usize) -> Result<KeyAgg, Musig2Error> {
        if party_index >= pks.len() {
            panic!("The is no party with index {}", party_index);
        }
        if pks.is_empty() {
            panic!("Not enough participant for multi-signature",);
        }
        let bn_1: PrivateKey = Scalar::from_int(1).into();
        let x_coor_vec = pks
            .iter()
            .map(|pk| PrivateKey::parse(&pk.x_coor()).expect("should be valid private key"))
            .collect::<Vec<_>>();

        let hash_vec: Vec<PrivateKey> = x_coor_vec
            .iter()
            .map(|pk| {
                let mut vec = vec![&bn_1, pk];
                for mpz in x_coor_vec.iter().take(pks.len()) {
                    vec.push(mpz);
                }
                let mut h = sha2::Sha256::default().tagged(b"BIP0340/challenge");
                for v in vec {
                    h = h.add(&v.clone());
                }
                let tagged = h.finalize();
                PrivateKey::parse_slice(tagged.as_slice()).expect("should be valid private key")
                // the "L" part of the hash
            })
            .collect();

        let X_tilde_vec: Vec<PublicKey> = pks
            .iter()
            .zip(&hash_vec)
            .map(|(pk, hash)| pk.mul_scalar(hash).expect("should be valid private key"))
            .collect();

        let sum = X_tilde_vec
            .iter()
            .skip(1)
            .fold(X_tilde_vec[0].clone(), |acc, pk| {
                acc.add_point(pk).expect("should be valid public key")
            });

        Ok(KeyAgg {
            X_tilde: sum,
            a_i: hash_vec[party_index].clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct EphemeralKey {
    pub keypair: KeyPair,
}

impl EphemeralKey {
    pub fn create_from_private_key(x1: &KeyPair, pad: usize) -> Result<EphemeralKey, Musig2Error> {
        // todo! exist issue may be not safe!

        let h = sha2::Sha256::default().tagged(b"BIP0340/challenge");
        let tagged = h
            .add(&x1.private_key)
            .add(&PrivateKey(Scalar::from_int(pad as u32)))
            .finalize();

        let hash_private_key_message = PrivateKey::parse_slice(tagged.as_slice())?;

        let ephemeral_private_key = hash_private_key_message;
        let ephemeral_public_key = PublicKey::create_from_private_key(&ephemeral_private_key);

        Ok(EphemeralKey {
            keypair: KeyPair {
                public_key: ephemeral_public_key,
                private_key: ephemeral_private_key,
            },
        })
    }

    pub fn create_vec_from_private_key(x1: &KeyPair) -> Result<Vec<EphemeralKey>, Musig2Error> {
        let mut EphemeralKeys_vec: Vec<EphemeralKey> = vec![];
        for i in 0..Nv {
            let eph_key = EphemeralKey::create_from_private_key(x1, i)?;
            EphemeralKeys_vec.push(eph_key);
        }
        Ok(EphemeralKeys_vec)
    }
}

pub fn sign(x: KeyPair) -> Result<(Vec<PublicKey>, State), Musig2Error> {
    let ephk_vec = EphemeralKey::create_vec_from_private_key(&x)?;
    let msg = ephk_vec
        .iter()
        .map(|eph_key| eph_key.clone().keypair.public_key)
        .collect();
    Ok((
        msg,
        State {
            keypair: x,
            ephk_vec,
        },
    ))
}

#[derive(Debug, Clone)]
pub struct State {
    pub keypair: KeyPair,
    pub ephk_vec: Vec<EphemeralKey>,
}

impl State {
    fn add_ephemeral_keys(&self, msg_vec: &[Vec<PublicKey>]) -> Vec<PublicKey> {
        let mut R_j_vec: Vec<PublicKey> = vec![];
        for j in 0..Nv {
            let pk_0j = self.ephk_vec[j].clone().keypair.public_key;
            //println!("{:?}", self.ephk_vec[j]);
            let R_j = msg_vec.iter().fold(pk_0j, |acc, ephk| {
                acc.add_point(&ephk[j]).expect("should be valid public key")
            });
            R_j_vec.push(R_j);
        }
        R_j_vec
    }

    fn compute_signature_share(
        &self,
        b_coefficients: &[PrivateKey],
        c: &PrivateKey,
        x: &KeyPair,
        a: &PrivateKey,
        is_odd: bool,
    ) -> Result<PrivateKey, Musig2Error> {
        let c_fe: PrivateKey = c.clone();
        let a_fe: PrivateKey = a.clone();
        let s = self.ephk_vec[0]
            .keypair
            .private_key
            .mul_scalar(&b_coefficients[0])?;
        let s = if is_odd { s.neg() } else { s };
        let lin_comb_ephemeral_i = self.ephk_vec.iter().zip(b_coefficients).skip(1).fold(
            Ok(s),
            |acc: Result<PrivateKey, Musig2Error>, (ephk, b)| {
                if is_odd {
                    Ok(acc?.add_scalar(&ephk.keypair.private_key.mul_scalar(b)?.neg())?)
                } else {
                    Ok(acc?.add_scalar(&ephk.keypair.private_key.mul_scalar(b)?)?)
                }
            },
        )?;
        lin_comb_ephemeral_i.add_scalar(&c_fe.mul_scalar(&x.private_key)?.mul_scalar(&a_fe)?)
    }

    // compute global parameters: c, R, and the b's coefficients
    pub fn compute_global_params(
        &self,
        message: &[u8],
        pks: &[PublicKey],
        msg_vec: Vec<Vec<PublicKey>>,
        party_index: usize,
    ) -> Result<(PrivateKey, PublicKey, Vec<PrivateKey>), Musig2Error> {
        let key_agg = KeyAgg::key_aggregation_n(pks, party_index)?;
        let R_j_vec = self.add_ephemeral_keys(&msg_vec);
        let mut b_coefficients: Vec<PrivateKey> = vec![PrivateKey(Scalar::from_int(1))];
        for j in 1..Nv {
            let mut hnon_preimage: Vec<PrivateKey> =
                vec![PrivateKey::parse_slice(&key_agg.X_tilde.x_coor())?];
            for i in R_j_vec.iter().take(Nv) {
                hnon_preimage.push(PrivateKey::parse_slice(&i.x_coor())?);
            }
            hnon_preimage.push(PrivateKey::parse_slice(message)?);
            hnon_preimage.push(PrivateKey(Scalar::from_int(j as u32)));
            let mut h = sha2::Sha256::default().tagged(b"BIP0340/challenge");
            for d in hnon_preimage.iter() {
                h = h.add(d)
            }
            let tagged = h.finalize();
            let b_j = PrivateKey::parse_slice(tagged.as_slice())?;
            b_coefficients.push(b_j);
        }
        let R_0 = R_j_vec[0].mul_scalar(&b_coefficients[0])?;
        let R = R_j_vec
            .iter()
            .zip(b_coefficients.clone())
            .skip(1)
            .map(|(R_j, b_j)| R_j.mul_scalar(&b_j))
            .fold(
                Ok(R_0),
                |acc: Result<PublicKey, Musig2Error>, R_j: Result<PublicKey, Musig2Error>| {
                    acc?.add_point(&R_j?)
                },
            )?;
        let rx1: PrivateKey = PrivateKey::parse_slice(&R.x_coor())?;
        let pkx1: PublicKey = key_agg.X_tilde;
        let msg1 = Message::parse_slice(message)?;
        let c = schnorrsig_challenge(&rx1, &pkx1, &msg1)?;
        Ok((c.into(), R, b_coefficients))
    }

    pub fn sign_prime(
        &self,
        message: &[u8],
        pks: &[PublicKey],
        msg_vec: Vec<Vec<PublicKey>>,
        party_index: usize,
    ) -> Result<(StatePrime, PrivateKey), Musig2Error> {
        let key_agg = KeyAgg::key_aggregation_n(pks, party_index)?;

        let (c, R, b_coefficients) =
            self.compute_global_params(message, pks, msg_vec, party_index)?;

        let is_odd = R.is_odd_y();
        let s_i =
            self.compute_signature_share(&b_coefficients, &c, &self.keypair, &key_agg.a_i, is_odd)?;
        Ok((
            StatePrime {
                R,
                s_i: s_i.clone(),
            },
            s_i,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct StatePrime {
    pub R: PublicKey,
    pub s_i: PrivateKey,
}

pub fn sign_double_prime(
    StatePrime: StatePrime,
    msg_vec: &[PrivateKey],
) -> Result<PrivateKey, Musig2Error> {
    let s_0 = StatePrime.s_i;
    msg_vec
        .iter()
        .fold(Ok(s_0), |acc: Result<PrivateKey, Musig2Error>, s_i| {
            acc?.add_scalar(s_i)
        })
}

/// Construct schnorr sig challenge
/// hash(R_x|P_x|msg)
pub fn schnorrsig_challenge(
    rx: &PrivateKey,
    pkx: &PublicKey,
    msg: &Message,
) -> Result<Scalar, Musig2Error> {
    let pkx = PrivateKey::parse(&pkx.x_coor())?;
    let mut bytes = [0u8; 32];
    let hash = sha2::Sha256::default().tagged(b"BIP0340/challenge");
    let tagged = hash.add(rx).add(&pkx).add(&msg.0).finalize();

    bytes.copy_from_slice(tagged.as_slice());
    let mut scalar = Scalar::default();
    let _ = scalar.set_b32(&bytes);
    Ok(scalar)
}

/// Verify a schnorr signature
pub fn verify(
    signature: &Signature,
    msg: &Message,
    pubkey: &PublicKey,
) -> Result<bool, Musig2Error> {
    let (rx, s) = (signature.rx.clone(), signature.s.clone());

    // Determine if the x coordinate is on the elliptic curve
    // Also here it will be verified that there are two y's at point x
    if PublicKey::parse_x_coor(&rx.serialize()).is_err() {
        return Err(Musig2Error::Invalid);
    }

    // Detect signature overflow
    let mut s_check = Scalar::default();
    let s_choice = s_check.set_b32(&s.serialize());
    if s_choice.unwrap_u8() == 1 {
        return Err(Invalid);
    }

    let P: Affine = pubkey.clone().into();

    if !P.is_valid_var() {
        return Err(Invalid);
    }

    let mut pj = Jacobian::default();
    pj.set_ge(&P);

    let pkx: PublicKey = P.into();

    let h = schnorrsig_challenge(&rx, &pkx, msg)?;

    let mut rj = Jacobian::default();
    ECMULT_CONTEXT.ecmult(&mut rj, &pj, &h.neg(), &s.into());

    let mut R = Affine::from_gej(&rj);

    if R.is_infinity() {
        return Err(Invalid);
    }

    R.y.normalize_var();

    if R.y.is_odd() {
        return Err(Invalid);
    }

    let mut rr = R.x;
    rr.normalize();

    // R = s⋅G - h⋅P, x(R) == rx
    if rx == PrivateKey::parse_slice(&rr.b32())? {
        Ok(true)
    } else {
        Err(Invalid)
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::protocals::signature::Signature;
    use core::convert::TryFrom;

    use super::*;

    /// BIP340 test vectors
    /// https://github.com/bitcoin/bips/blob/master/bip-0340/test-vectors.csv
    const PUBKEY_4: &str = "D69C3509BB99E412E68B0FE8544E72837DFA30746D8BE2AA65975F29D22DC7B9";

    const MESSAGE_4: &str = "4DF3C3F68FCC83B27E9D42C90431A72499F17875C81A599B566C9889B9696703";

    const SIGNATURE_4: &str = "00000000000000000000003B78CE563F89A0ED9414F5AA28AD0D96D6795F9C6376AFB1548AF603B3EB45C9F8207DEE1060CB71C04E80F593060B07D28308D7F4";

    fn check_verify(sig: &str, msg: &str, pubkey: &str) -> bool {
        let s = Signature::try_from(sig).unwrap();

        let pk = PublicKey::try_from(pubkey).unwrap();
        let m = Message::parse_slice(&hex::decode(msg).unwrap()[..]).unwrap();

        verify(&s, &m, &pk).unwrap()
    }

    #[test]
    fn test_verify() {
        assert!(check_verify(SIGNATURE_4, MESSAGE_4, PUBKEY_4));
    }
}
