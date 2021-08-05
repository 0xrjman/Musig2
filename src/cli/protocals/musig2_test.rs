#[allow(non_snake_case)]
#[test]
fn test_multiparty_signing_for_two_parties() {
    use crate::cli::protocals::musig2::*;
    use curv::elliptic::curves::{secp256_k1::GE, traits::ECPoint};

    let message: [u8; 4] = [79, 77, 69, 82];

    // round 0: generate signing keys
    let party1_key = KeyPair::create();
    let party2_key = KeyPair::create();
    let mut pks: Vec<GE> = Vec::new();
    pks.push(party1_key.public_key.clone());
    pks.push(party2_key.public_key.clone());

    // compute X_tilde:
    let party1_key_agg = KeyAgg::key_aggregation_n(&pks, 0);
    let party2_key_agg = KeyAgg::key_aggregation_n(&pks, 1);
    assert_eq!(party1_key_agg.X_tilde, party2_key_agg.X_tilde);

    //Sign: each party creates state that contains a vector of ephemeral keys
    let (party_1_msg_round_1, party_1_state) = sign(party1_key);
    let (party_2_msg_round_1, party_2_state) = sign(party2_key);

    let party1_received_msg_round_1 = vec![Vec::from(party_2_msg_round_1)];
    let party2_received_msg_round_1 = vec![Vec::from(party_1_msg_round_1)];

    //Sign prime: each party creates state'
    let (party_1_state_prime, party1_msg_round_2) =
        party_1_state.sign_prime(&message, &pks, party1_received_msg_round_1.clone(), 0);
    let (party_2_state_prime, party2_msg_round_2) =
        party_2_state.sign_prime(&message, &pks, party2_received_msg_round_1.clone(), 1);

    //round 2: sending signature shares
    let party1_received_msg_round_2 = vec![party2_msg_round_2];
    let party2_received_msg_round_2 = vec![party1_msg_round_2];

    //Constructing state'' by combining the signature shares
    let s_total_1 = sign_double_prime(party_1_state_prime, &party1_received_msg_round_2);
    let s_total_2 = sign_double_prime(party_2_state_prime, &party2_received_msg_round_2);
    //verify that both parties computed the same signature
    assert_eq!(s_total_1, s_total_2);
    let s = s_total_1;

    // Computing global parameters c and R for verification
    let (c_party_1, R_party_1, _) =
        party_1_state.compute_global_params(&message, &pks, party1_received_msg_round_1.clone(), 0);
    let (c_party_2, R_party_2, _) =
        party_2_state.compute_global_params(&message, &pks, party2_received_msg_round_1.clone(), 1);

    //Verify that they both computed the same values
    assert_eq!(R_party_1, R_party_2);
    assert_eq!(c_party_1, c_party_2);
    let R = R_party_1;
    let c = c_party_1;

    // verification that the signature is computed correctly
    assert!(verify(&s, &R.x_coor().unwrap(), &party1_key_agg.X_tilde, &c).is_ok());
}
