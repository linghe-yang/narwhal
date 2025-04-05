use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use crypto::{PublicKey, Signature};
use crate::breeze_structs::BreezeCertificate;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumboMessage{
    pub sender: PublicKey,
    pub content: DumboContent
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DumboContent {
    Certificate(BreezeCertificate),
    // Propose(HashSet<BreezeCertificate>),
    Vote((HashSet<BreezeCertificate>,Signature)),
    Decided((HashSet<BreezeCertificate>,HashSet<(PublicKey,Signature)>)),
}