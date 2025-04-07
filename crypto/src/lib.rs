
use std::array::TryFromSliceError;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::oneshot;
#[cfg(test)]
#[cfg(not(feature = "pq"))]
#[path = "tests/crypto_tests.rs"]
pub mod crypto_tests; // only the test for ed25519
#[cfg(not(feature = "pq"))]
use ed25519_dalek::ed25519;
#[cfg(not(feature = "pq"))]
pub use ed25519_sig::*;
#[cfg(not(feature = "pq"))]
pub(crate) mod ed25519_sig;
#[cfg(not(feature = "pq"))]
pub type CryptoError = ed25519::Error;

#[cfg(feature = "pq")]
pub(crate) mod dilithum_sig;

#[cfg(feature = "pq")]
pub use dilithum_sig::*;
#[cfg(feature = "pq")]
#[derive(Debug)]
pub enum CryptoError {
    InvalidSignature
}
#[cfg(feature = "pq")]
impl std::error::Error for CryptoError {}
#[cfg(feature = "pq")]
impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::InvalidSignature => write!(f, "Invalid signature"),
        }
    }
}


/// Represents a hash digest (32 bytes).
#[derive(Copy, Hash, PartialEq, Default, Eq, Clone, Deserialize, Serialize, Ord, PartialOrd)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn size(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", base64::encode(&self.0).get(0..16).unwrap())
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Digest {
    type Error = TryFromSliceError;
    fn try_from(item: &[u8]) -> Result<Self, Self::Error> {
        Ok(Digest(item.try_into()?))
    }
}

/// This trait is implemented by all messages that can be hashed.
pub trait Hash {
    fn digest(&self) -> Digest;
}

/// This service holds the node's private key. It takes digests as input and returns a signature
/// over the digest (through a oneshot channel).
#[derive(Clone)]
pub struct SignatureService {
    channel: Sender<(Digest, oneshot::Sender<Signature>)>,
}
impl SignatureService {
    pub fn new(secret: SecretKey) -> Self {
        let (tx, mut rx): (Sender<(_, oneshot::Sender<_>)>, _) = channel(100);
        tokio::spawn(async move {
            while let Some((digest, sender)) = rx.recv().await {
                let signature = Signature::new(&digest, &secret);
                let _ = sender.send(signature);
            }
        });
        Self { channel: tx }
    }

    pub async fn request_signature(&mut self, digest: Digest) -> Signature {
        let (sender, receiver): (oneshot::Sender<_>, oneshot::Receiver<_>) = oneshot::channel();
        if let Err(e) = self.channel.send((digest, sender)).await {
            panic!("Failed to send message Signature Service: {}", e);
        }
        receiver
            .await
            .expect("Failed to receive signature from Signature Service")
    }
}