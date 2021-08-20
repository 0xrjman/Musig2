use hex::FromHexError;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Musig2Error {
    #[allow(dead_code)]
    Invalid,

    InvalidSignature,
    InvalidPublicKey,
    InvalidPrivateKey,
    InvalidRecoveryId,
    InvalidMessage,
    InvalidInputLength,
    TweakOutOfRange,

    InvalidHexCharacter,
    InvalidStringLength,
    OddLength,
    XCoordinateNotExist,
}

impl From<secp256k1::Error> for Musig2Error {
    fn from(e: secp256k1::Error) -> Self {
        match e {
            secp256k1::Error::InvalidSignature => Musig2Error::InvalidSignature,
            secp256k1::Error::InvalidPublicKey => Musig2Error::InvalidPublicKey,
            secp256k1::Error::InvalidSecretKey => Musig2Error::InvalidPrivateKey,
            secp256k1::Error::InvalidRecoveryId => Musig2Error::InvalidRecoveryId,
            secp256k1::Error::InvalidMessage => Musig2Error::InvalidMessage,
            secp256k1::Error::InvalidInputLength => Musig2Error::InvalidInputLength,
            secp256k1::Error::TweakOutOfRange => Musig2Error::TweakOutOfRange,
        }
    }
}

impl From<FromHexError> for Musig2Error {
    fn from(e: FromHexError) -> Self {
        match e {
            FromHexError::InvalidHexCharacter { .. } => Musig2Error::InvalidHexCharacter,
            FromHexError::InvalidStringLength => Musig2Error::InvalidStringLength,
            FromHexError::OddLength => Musig2Error::OddLength,
        }
    }
}
