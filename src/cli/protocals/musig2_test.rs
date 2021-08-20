#[cfg(test)]
mod tests1 {
    use secp256k1::Message;

    use crate::cli::protocals::key::{PrivateKey, PublicKey};

    #[allow(non_snake_case)]
    #[test]
    fn test_multiparty_signing_for_two_parties2() {
        use crate::cli::protocals::musig2::*;
        use crate::cli::protocals::signature::Signature;
        use rand_core::{OsRng, RngCore};
        for i in 0..100 {
            println!("-----------{}-----------", i);
            let mut message: [u8; 32] = [0u8; 32];
            OsRng.fill_bytes(&mut message);

            // round 0: generate signing keys
            let party1_key = KeyPair::create().unwrap();
            let party2_key = KeyPair::create().unwrap();
            let mut pks: Vec<PublicKey> = Vec::new();
            pks.push(party1_key.public_key.clone());
            pks.push(party2_key.public_key.clone());

            // compute X_tilde:
            let party1_key_agg = KeyAgg::key_aggregation_n(&pks, 0).unwrap();
            let party2_key_agg = KeyAgg::key_aggregation_n(&pks, 1).unwrap();
            assert_eq!(party1_key_agg.X_tilde, party2_key_agg.X_tilde);

            //Sign: each party creates state that contains a vector of ephemeral keys
            let (party_1_msg_round_1, party_1_state) = sign(party1_key).unwrap();
            let (party_2_msg_round_1, party_2_state) = sign(party2_key).unwrap();

            let party1_received_msg_round_1 = vec![Vec::from(party_2_msg_round_1)];
            let party2_received_msg_round_1 = vec![Vec::from(party_1_msg_round_1)];

            //Sign prime: each party creates state'
            let (party_1_state_prime, party1_msg_round_2) = party_1_state
                .sign_prime(&message, &pks, party1_received_msg_round_1.clone(), 0)
                .unwrap();
            let (party_2_state_prime, party2_msg_round_2) = party_2_state
                .sign_prime(&message, &pks, party2_received_msg_round_1.clone(), 1)
                .unwrap();

            //round 2: sending signature shares
            let party1_received_msg_round_2 = vec![party2_msg_round_2];
            let party2_received_msg_round_2 = vec![party1_msg_round_2];

            //Constructing state'' by combining the signature shares
            let s_total_1 =
                sign_double_prime(party_1_state_prime, &party1_received_msg_round_2).unwrap();
            let s_total_2 =
                sign_double_prime(party_2_state_prime, &party2_received_msg_round_2).unwrap();
            //verify that both parties computed the same signature
            assert_eq!(s_total_1, s_total_2);
            let s = s_total_1;

            // Computing global parameters c and R for verification
            let (c_party_1, R_party_1, _) = party_1_state
                .compute_global_params(&message, &pks, party1_received_msg_round_1.clone(), 0)
                .unwrap();
            let (c_party_2, R_party_2, _) = party_2_state
                .compute_global_params(&message, &pks, party2_received_msg_round_1.clone(), 1)
                .unwrap();

            //Verify that they both computed the same values
            assert_eq!(R_party_1, R_party_2);
            assert_eq!(c_party_1, c_party_2);

            let R = R_party_1;
            let _c = c_party_1;
            let signature = Signature {
                rx: PrivateKey::parse_slice(&R.x_coor()).unwrap(),
                s,
            };
            assert!(verify(
                &signature,
                &Message::parse_slice(&message).unwrap(),
                &party1_key_agg.X_tilde
            )
            .unwrap());
        }
    }
}
