use std::collections::{BTreeSet, HashSet};
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
    Vote((BTreeSet<BreezeCertificate>,Signature)),
    Decided((BTreeSet<BreezeCertificate>,HashSet<(PublicKey,Signature)>)),
}