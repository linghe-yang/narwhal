use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::{fmt};
use pqcrypto_dilithium::dilithium2::{keypair, open, sign, PublicKey as DilithiumPublicKey, SecretKey as DilithiumSecretKey, SignedMessage};
use pqcrypto_traits::sign::{PublicKey as DilithiumPublicKeyTrait, SecretKey as DilithiumSecretKeyTrait, SignedMessage as DilithiumSignedMessage};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as ShaDigest, Sha256};
use crate::{CryptoError, Digest};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq)]
pub struct PublicKey(DilithiumPublicKey);

impl PublicKey {

    pub fn new_random_test() -> PublicKey {
        let (pk,_) = keypair();
        PublicKey(pk)
    }
    pub fn to_hash32(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        let result = hasher.finalize();
        result.into()
    }
}

impl Eq for PublicKey {}

impl Hash for PublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other)) 
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes()) 
    }
}
impl Default for PublicKey {
    fn default() -> Self {
        let zeroed_data: [u8; 1312] = [0; 1312];
        let pk = DilithiumPublicKey::from_bytes(&zeroed_data)
            .expect("Failed to create default PublicKey from zeroed bytes");
        PublicKey(pk)
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = base64::encode(self.0.as_bytes());
        serializer.serialize_str(&encoded)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = base64::decode(&encoded)
            .map_err(serde::de::Error::custom)?;
        let dilithium_pk = DilithiumPublicKey::from_bytes(&bytes)
            .map_err(serde::de::Error::custom)?;
        Ok(PublicKey(dilithium_pk))
    }
}

// 实现Display trait
impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0.as_bytes();
        write!(f, "PublicKey({})", hex::encode(bytes))
    }
}

// 实现Debug trait
impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0.as_bytes();
        f.debug_struct("PublicKey")
            .field("data", &format_args!("0x{}", hex::encode(bytes)))
            .finish()
    }
}
impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_bytes()
    }
}

#[repr(transparent)]
#[derive(Clone)]
pub struct SecretKey(DilithiumSecretKey);
impl Serialize for SecretKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = base64::encode(self.0.as_bytes());
        serializer.serialize_str(&encoded)
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = base64::decode(&encoded)
            .map_err(serde::de::Error::custom)?;
        let dilithium_sk = DilithiumSecretKey::from_bytes(&bytes)
            .map_err(serde::de::Error::custom)?;
        Ok(SecretKey(dilithium_sk))
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0.as_bytes();
        write!(f, "SecretKey({})", hex::encode(bytes))
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0.as_bytes();
        f.debug_struct("SecretKey")
            .field("data", &format_args!("0x{}", hex::encode(bytes)))
            .finish()
    }
}

pub fn generate_production_keypair() -> (PublicKey, SecretKey) {
    generate_keypair()
}
pub fn generate_keypair() -> (PublicKey, SecretKey){
    let (pk,sk) = keypair();
    (PublicKey(pk), SecretKey(sk))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Signature(SignedMessage);

impl Default for Signature {
    fn default() -> Self {
        let empty_signed_message = SignedMessage::from_bytes(&[])
            .expect("Failed to create empty SignedMessage"); 
        Signature(empty_signed_message)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signature")
            .field("data", &self.0.as_bytes()) 
            .finish()
    }
}

impl Eq for Signature {}

// 实现PartialEq
impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes() == other.0.as_bytes() 
    }
}
impl Ord for Signature {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl PartialOrd for Signature {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other)) 
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state); 
    }
}

impl Signature {
    pub fn new(digest: &Digest, secret: &SecretKey) -> Self {
        Signature(sign(digest.as_ref(), &secret.0))
    }
    pub fn verify(&self, digest: &Digest, public_key: &PublicKey) -> Result<(), CryptoError> {
        let opened_msg = open(&self.0, &public_key.0).unwrap();
        if digest.to_vec() == opened_msg {
            return Ok(());
        }
        Err(CryptoError::InvalidSignature)

    }
    pub fn verify_batch<'a, I>(digest: &Digest, votes: I) -> Result<(), CryptoError>
    where
        I: IntoIterator<Item = &'a (PublicKey, Signature)>,
    {
        for (key, sig) in votes.into_iter() {
            sig.verify(digest, key)?;
        }
        Ok(())
    }
}