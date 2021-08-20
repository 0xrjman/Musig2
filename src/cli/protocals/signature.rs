//! This is 64-byte schnorr signature.
//!
//! More details:
//! [`BIP340`]: https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki#design
use super::{error::Musig2Error, key::PrivateKey};
use core::convert::{TryFrom, TryInto};

/// A standard for 64-byte Schnorr signatures over the elliptic curve secp256k1
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Signature {
    pub rx: PrivateKey,
    pub s: PrivateKey,
}

impl TryFrom<&str> for Signature {
    type Error = Musig2Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut s_slice = [0u8; 64];
        s_slice.copy_from_slice(&hex::decode(value).unwrap()[..]);
        s_slice.try_into()
    }
}

impl TryFrom<[u8; 64]> for Signature {
    type Error = Musig2Error;

    fn try_from(bytes: [u8; 64]) -> Result<Self, Self::Error> {
        let mut rx_bytes = [0u8; 32];
        rx_bytes.copy_from_slice(&bytes[0..32]);

        let mut s_bytes = [0u8; 32];
        s_bytes.copy_from_slice(&bytes[32..64]);

        let rx = PrivateKey::parse(&rx_bytes)?;
        let s = PrivateKey::parse(&s_bytes)?;
        Ok(Signature { rx, s })
    }
}
