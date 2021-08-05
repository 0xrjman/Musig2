use libp2p::{
    core::{
        muxing::StreamMuxerBox,
        transport::{upgrade::Version, Boxed},
    },
    identity,
    mplex::MplexConfig,
    noise::{self, NoiseConfig},
    tcp::TokioTcpConfig,
    PeerId, Transport,
};

/// Transport type.
pub(crate) type TTransport = Boxed<(PeerId, StreamMuxerBox)>;

/// Builds the transport that serves as a common ground for all connections.
///
/// Set up an encrypted TCP transport over the Mplex protocol.
pub fn build_transport(keypair: identity::Keypair) -> TTransport {
    let xx_keypair = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&keypair)
        .unwrap();
    let noise_config = NoiseConfig::xx(xx_keypair).into_authenticated();

    TokioTcpConfig::new()
        .upgrade(Version::V1)
        .authenticate(noise_config)
        .multiplex(MplexConfig::new())
        .boxed()
}
