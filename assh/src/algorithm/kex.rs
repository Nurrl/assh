use secrecy::{ExposeSecret, SecretBox};
use signature::{SignatureEncoding, Signer, Verifier};
use ssh_key::{PrivateKey, Signature};
use ssh_packet::{
    arch::MpInt,
    crypto::exchange,
    trans::{KexEcdhInit, KexEcdhReply, KexInit},
    Id,
};
use strum::{AsRefStr, EnumString};

use crate::{
    stream::{Keys, Stream, Transport, TransportPair},
    Error, Pipe, Result,
};

use super::{cipher, compress, hmac};

pub fn negociate(clientkex: &KexInit, serverkex: &KexInit) -> Result<Kex> {
    clientkex
        .kex_algorithms
        .preferred_in(&serverkex.kex_algorithms)
        .ok_or(Error::NoCommonKex)?
        .parse()
        .map_err(|_| Error::NoCommonKex)
}

// TODO: (feature) Implement the following legacy key-exchange methods (`diffie-hellman-group14-sha256`, `diffie-hellman-group14-sha1`, `diffie-hellman-group1-sha1`).

/// SSH key-exchange algorithms.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Kex {
    /// Curve25519 ECDH with sha-2-256 digest.
    Curve25519Sha256,

    /// Curve25519 ECDH with sha-2-256 digest (pre-RFC 8731).
    #[strum(serialize = "curve25519-sha256@libssh.org")]
    Curve25519Sha256Libssh,
    //
    // DiffieHellmanGroup14Sha256,
    //
    // DiffieHellmanGroup14Sha1,
    //
    // DiffieHellmanGroup1Sha1,
}

impl Kex {
    pub(crate) async fn init<S: Pipe>(
        &self,
        stream: &mut Stream<S>,
        v_c: &Id,
        v_s: &Id,
        i_c: KexInit<'_>,
        i_s: KexInit<'_>,
    ) -> Result<TransportPair> {
        let (client_hmac, server_hmac) = hmac::negociate(&i_c, &i_s)?;
        let (client_compress, server_compress) = compress::negociate(&i_c, &i_s)?;
        let (client_cipher, server_cipher) = cipher::negociate(&i_c, &i_s)?;

        match self {
            Self::Curve25519Sha256 | Self::Curve25519Sha256Libssh => {
                type Hash = sha2::Sha256;

                let e_c = x25519_dalek::EphemeralSecret::random_from_rng(rand::thread_rng());
                let q_c = x25519_dalek::PublicKey::from(&e_c);

                stream
                    .send(&KexEcdhInit {
                        q_c: q_c.as_ref().into(),
                    })
                    .await?;

                let ecdh: KexEcdhReply = stream.recv().await?.to()?;
                let q_s = x25519_dalek::PublicKey::from(
                    <[u8; 32]>::try_from(&*ecdh.q_s).map_err(|_| Error::KexError)?,
                );

                let secret = e_c.diffie_hellman(&q_s);
                let secret = SecretBox::new(MpInt::positive(secret.as_bytes()).into());

                let k_s = ssh_key::PublicKey::from_bytes(&ecdh.k_s)?;
                let hash = exchange::Ecdh {
                    v_c: v_c.to_string().into_bytes().into(),
                    v_s: v_s.to_string().into_bytes().into(),
                    i_c: (&i_c).into(),
                    i_s: (&i_s).into(),
                    k_s: ecdh.k_s,
                    q_c: q_c.as_ref().into(),
                    q_s: q_s.as_ref().into(),
                    k: secret.expose_secret().as_borrow(),
                }
                .hash::<Hash>();

                Verifier::verify(&k_s, &hash, &Signature::try_from(ecdh.signature.as_ref())?)?;

                let session_id = stream.with_session(&hash);

                Ok(TransportPair {
                    rx: Transport {
                        chain: Keys::as_server::<Hash>(
                            secret.expose_secret(),
                            &hash,
                            session_id,
                            &client_cipher,
                            &client_hmac,
                        ),
                        state: None,
                        cipher: client_cipher,
                        hmac: client_hmac,
                        compress: client_compress,
                    },
                    tx: Transport {
                        chain: Keys::as_client::<Hash>(
                            secret.expose_secret(),
                            &hash,
                            session_id,
                            &server_cipher,
                            &server_hmac,
                        ),
                        state: None,
                        cipher: server_cipher,
                        hmac: server_hmac,
                        compress: server_compress,
                    },
                })
            }
        }
    }

    pub(crate) async fn reply<S: Pipe>(
        &self,
        stream: &mut Stream<S>,
        v_c: &Id,
        v_s: &Id,
        i_c: KexInit<'_>,
        i_s: KexInit<'_>,
        key: &PrivateKey,
    ) -> Result<TransportPair> {
        let (client_hmac, server_hmac) = hmac::negociate(&i_c, &i_s)?;
        let (client_compress, server_compress) = compress::negociate(&i_c, &i_s)?;
        let (client_cipher, server_cipher) = cipher::negociate(&i_c, &i_s)?;

        match self {
            Self::Curve25519Sha256 | Self::Curve25519Sha256Libssh => {
                type Hash = sha2::Sha256;

                let ecdh: KexEcdhInit = stream.recv().await?.to()?;

                let e_s = x25519_dalek::EphemeralSecret::random_from_rng(rand::thread_rng());
                let q_s = x25519_dalek::PublicKey::from(&e_s);

                let q_c = x25519_dalek::PublicKey::from(
                    <[u8; 32]>::try_from(ecdh.q_c.as_ref()).map_err(|_| Error::KexError)?,
                );

                let secret = e_s.diffie_hellman(&q_c);
                let secret = SecretBox::new(MpInt::positive(secret.as_bytes()).into());

                let k_s = key.public_key().to_bytes()?;

                let hash = exchange::Ecdh {
                    v_c: v_c.to_string().into_bytes().into(),
                    v_s: v_s.to_string().into_bytes().into(),
                    i_c: (&i_c).into(),
                    i_s: (&i_s).into(),
                    k_s: k_s.as_slice().into(),
                    q_c: q_c.as_ref().into(),
                    q_s: q_s.as_ref().into(),
                    k: secret.expose_secret().as_borrow(),
                }
                .hash::<Hash>();

                let signature = Signer::sign(key, &hash);

                stream
                    .send(&KexEcdhReply {
                        k_s: k_s.into(),
                        q_s: q_s.as_ref().into(),
                        signature: signature.to_vec().into(),
                    })
                    .await?;

                let session_id = stream.with_session(&hash);

                Ok(TransportPair {
                    rx: Transport {
                        chain: Keys::as_client::<Hash>(
                            secret.expose_secret(),
                            &hash,
                            session_id,
                            &client_cipher,
                            &client_hmac,
                        ),
                        state: None,
                        cipher: client_cipher,
                        hmac: client_hmac,
                        compress: client_compress,
                    },
                    tx: Transport {
                        chain: Keys::as_server::<Hash>(
                            secret.expose_secret(),
                            &hash,
                            session_id,
                            &server_cipher,
                            &server_hmac,
                        ),
                        state: None,
                        cipher: server_cipher,
                        hmac: server_hmac,
                        compress: server_compress,
                    },
                })
            }
        }
    }
}
