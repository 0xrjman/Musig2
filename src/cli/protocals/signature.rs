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
// TODO: add support to bip340

#![allow(non_snake_case)]

use curv::arithmetic::traits::Converter;
use curv::cryptographic_primitives::commitments::hash_commitment::HashCommitment;
use curv::cryptographic_primitives::commitments::traits::*;
use curv::cryptographic_primitives::hashing::hash_sha256::HSha256;
use curv::cryptographic_primitives::hashing::traits::*;
use curv::cryptographic_primitives::proofs::*;
use curv::elliptic::curves::traits::*;
use curv::BigInt;

use serde::{Deserialize, Serialize};

pub type GE = curv::elliptic::curves::secp256_k1::GE;
pub type FE = curv::elliptic::curves::secp256_k1::FE;

#[allow(non_upper_case_globals)]
const Nv: usize = 2;

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub public_key: GE,
    private_key: FE,
}

impl KeyPair {
    pub fn create() -> KeyPair {
        let ec_point: GE = ECPoint::generator();
        let private_key: FE = ECScalar::new_random();
        let public_key = ec_point.scalar_mul(&private_key.get_element());
        KeyPair {
            public_key,
            private_key,
        }
    }

    // pub fn create_from_private_key(private_key: &BigInt) -> KeyPair {
    //     let ec_point: GE = ECPoint::generator();
    //     let private_key: FE = ECScalar::from(private_key);
    //     let public_key = ec_point.scalar_mul(&private_key.get_element());
    //     KeyPair {
    //         public_key,
    //         private_key,
    //     }
    // }
}

#[derive(Debug, Clone)]
pub struct KeyAgg {
    pub X_tilde: GE,
    pub a_i: BigInt,
}

impl KeyAgg {
    pub fn key_aggregation_n(pks: &[GE], party_index: usize) -> KeyAgg {
        if party_index >= pks.len() {
            panic!("The is no party with index {}", party_index);
        }
        if pks.is_empty() {
            panic!("Not enough participant for multi-signature",);
        }
        let bn_1 = BigInt::from(1);
        let x_coor_vec: Vec<BigInt> = pks
            .iter()
            .map(|pk| pk.bytes_compressed_to_big_int())
            .collect();

        let hash_vec: Vec<BigInt> = x_coor_vec
            .iter()
            .map(|pk| {
                let mut vec = vec![&bn_1, pk];
                for mpz in x_coor_vec.iter().take(pks.len()) {
                    vec.push(mpz);
                }
                HSha256::create_hash(&vec)
                // the "L" part of the hash
            })
            .collect();

        let X_tilde_vec: Vec<GE> = pks
            .iter()
            .zip(&hash_vec)
            .map(|(pk, hash)| {
                let hash_t: FE = ECScalar::from(hash);
                let pki: GE = *pk;
                pki.scalar_mul(&hash_t.get_element())
            })
            .collect();

        let sum = X_tilde_vec
            .iter()
            .skip(1)
            .fold(X_tilde_vec[0], |acc, pk| acc.add_point(&pk.get_element()));

        KeyAgg {
            X_tilde: sum,
            a_i: hash_vec[party_index].clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EphemeralKey {
    pub keypair: KeyPair,
    pub commitment: BigInt,
    pub blind_factor: BigInt,
}

impl EphemeralKey {
    pub fn create_from_private_key(x1: &KeyPair, pad: usize) -> EphemeralKey {
        let base_point: GE = ECPoint::generator();
        let hash_private_key_message =
            HSha256::create_hash(&[&x1.private_key.to_big_int(), &BigInt::from(pad as i32)]);
        let ephemeral_private_key: FE = ECScalar::from(&hash_private_key_message);
        let ephemeral_public_key = base_point.scalar_mul(&ephemeral_private_key.get_element());
        let (commitment, blind_factor) =
            HashCommitment::create_commitment(&ephemeral_public_key.bytes_compressed_to_big_int());
        EphemeralKey {
            keypair: KeyPair {
                public_key: ephemeral_public_key,
                private_key: ephemeral_private_key,
            },
            commitment,
            blind_factor,
        }
    }

    pub fn create_vec_from_private_key(x1: &KeyPair) -> Vec<EphemeralKey> {
        let mut EphemeralKeys_vec: Vec<EphemeralKey> = vec![];
        for i in 0..Nv {
            let eph_key = EphemeralKey::create_from_private_key(x1, i);
            EphemeralKeys_vec.push(eph_key);
        }
        EphemeralKeys_vec
    }
}

// pub fn hash_tag_challange(r_hat: &GE, X_tilde: &GE) -> BigInt {
//     HSha256::create_hash(&[
//         &r_hat.x_coor().unwrap(),
//         &X_tilde.bytes_compressed_to_big_int(),
//     ])
// }

pub fn hash_tag(r_hat: &GE, X_tilde: &GE) -> BigInt {
    HSha256::create_hash(&[
        &BigInt::from(0),
        &r_hat.x_coor().unwrap(),
        &X_tilde.bytes_compressed_to_big_int(),
    ])
}

pub fn sign(x: KeyPair) -> (Vec<GE>, State) {
    let ephk_vec = EphemeralKey::create_vec_from_private_key(&x);
    let msg = ephk_vec
        .iter()
        .map(|eph_key| eph_key.keypair.public_key)
        .collect();
    (
        msg,
        State {
            keypair: x,
            ephk_vec,
        },
    )
}

#[derive(Debug, Clone)]
pub struct State {
    pub keypair: KeyPair,
    pub ephk_vec: Vec<EphemeralKey>,
}

impl State {
    fn add_ephemeral_keys(&self, msg_vec: &[Vec<GE>]) -> Vec<GE> {
        let mut R_j_vec: Vec<GE> = vec![];
        for j in 0..Nv {
            let pk_0j = self.ephk_vec[j].keypair.public_key;
            //println!("{:?}", self.ephk_vec[j]);
            let R_j: GE = msg_vec.iter().fold(pk_0j, |acc, ephk| {
                acc.add_point(&ephk.get(j).unwrap().get_element())
            });
            R_j_vec.push(R_j);
        }
        R_j_vec
    }

    fn compute_signature_share(
        &self,
        b_coefficients: &[BigInt],
        c: &BigInt,
        x: &KeyPair,
        a: &BigInt,
    ) -> FE {
        let c_fe: FE = ECScalar::from(c);
        let a_fe: FE = ECScalar::from(a);
        let lin_comb_ephemeral_i: FE = self
            .ephk_vec
            .iter()
            .zip(b_coefficients)
            .fold(ECScalar::zero(), |acc, (ephk, b)| {
                acc + ephk.keypair.private_key * <FE as ECScalar>::from(b)
            });
        lin_comb_ephemeral_i + (c_fe * x.private_key * a_fe)
    }

    // compute global parameters: c, R, and the b's coefficients
    pub fn compute_global_params(
        &self,
        message: &[u8],
        pks: &[GE],
        msg_vec: Vec<Vec<GE>>,
        party_index: usize,
    ) -> (BigInt, GE, Vec<BigInt>) {
        let key_agg = KeyAgg::key_aggregation_n(pks, party_index);
        let R_j_vec = self.add_ephemeral_keys(&msg_vec);
        let mut b_coefficients: Vec<BigInt> = vec![BigInt::from(1)];
        for j in 1..Nv {
            let mut hnon_preimage: Vec<BigInt> =
                vec![key_agg.X_tilde.bytes_compressed_to_big_int()];
            for i in R_j_vec.iter().take(Nv) {
                hnon_preimage.push(i.bytes_compressed_to_big_int());
            }
            hnon_preimage.push(BigInt::from_bytes(message));
            hnon_preimage.push(BigInt::from(j as i32));
            let b_j = HSha256::create_hash(&hnon_preimage.iter().collect::<Vec<_>>());
            b_coefficients.push(b_j);
        }

        let R_0 = R_j_vec[0] * <FE as ECScalar>::from(&b_coefficients[0]);
        let R: GE = R_j_vec
            .iter()
            .zip(b_coefficients.clone())
            .skip(1)
            .map(|(R_j, b_j)| R_j * &<FE as ECScalar>::from(&b_j))
            .fold(R_0, |acc, R_j| acc.add_point(&R_j.get_element()));
        let c = hash_tag(&R, &key_agg.X_tilde);
        (c, R, b_coefficients)
    }

    pub fn sign_prime(
        &self,
        message: &[u8],
        pks: &[GE],
        msg_vec: Vec<Vec<GE>>,
        party_index: usize,
    ) -> (StatePrime, FE) {
        let key_agg = KeyAgg::key_aggregation_n(pks, party_index);
        let (c, R, b_coefficients) = self.compute_global_params(message, pks, msg_vec, party_index);
        let s_i = self.compute_signature_share(&b_coefficients, &c, &self.keypair, &key_agg.a_i);
        (StatePrime { R, s_i }, s_i)
    }
}

#[derive(Debug, Clone)]
pub struct StatePrime {
    pub R: GE,
    pub s_i: FE,
}

pub fn sign_double_prime(StatePrime: StatePrime, msg_vec: &[FE]) -> FE {
    let s_0 = StatePrime.s_i;
    msg_vec.iter().fold(s_0, |acc, s_i| acc + s_i)
}

pub fn verify(
    signature: &FE,
    r_x: &BigInt,
    X_tilde: &GE,
    c: &BigInt, //musig_bit: bool,
) -> Result<(), ProofError> {
    let base_point: GE = ECPoint::generator();
    let sG = base_point.scalar_mul(&signature.get_element());
    let c: FE = ECScalar::from(c);
    let cY = X_tilde.scalar_mul(&c.get_element());
    let sG = sG.sub_point(&cY.get_element());
    if sG.x_coor().unwrap().to_hex() == r_x.to_hex() {
        Ok(())
    } else {
        Err(ProofError)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Round1 {
    pub topic: String,
    pub sender: Vec<u8>,
    pub num: usize,
    pub ephemeral_keys: Vec<GE>,
    pub msg: Vec<u8>,
    pub pubkey: GE,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Round2 {
    pub topic: String,
    pub sender: Vec<u8>,
    pub sign: FE,
}
