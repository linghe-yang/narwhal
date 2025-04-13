use super::breeze_confirm::BreezeConfirm;
use super::breeze_message_handler::BreezeMessageHandler;
use super::breeze_reconstruct::BreezeReconstruct;
use super::breeze_reply::BreezeReply;
use super::breeze_result::BreezeResult;
use super::breeze_share::BreezeShare;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use network::{Receiver as NetworkReceiver, ReliableSender};
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::{watch, RwLock};
use config::{Committee, KeyPair};
use model::breeze_universal::{BreezeCertificate, BreezeReconRequest, CommonReferenceString};
use model::types_and_const::{Epoch, Id, RandomNum, CHANNEL_CAPACITY};
use crypto::{Digest, PublicKey};
use crate::breeze_structs::{BreezeMessage, PQCrs};

pub struct Breeze;

impl Breeze {
    pub fn spawn(
        keypair: KeyPair,
        address: SocketAddr,
        id:Id,
        // committee: Arc<RwLock<Committee>>,
        committee: Committee,
        breeze_share_cmd_receiver: Receiver<Epoch>,
        breeze_certificate_sender: Sender<BreezeCertificate>,
        breeze_reconstruct_cmd_receiver: Receiver<BreezeReconRequest>,
        breeze_result_sender: Sender<(Epoch, usize, RandomNum)>,

        common_reference_string: CommonReferenceString,
    ) {
        let pk = keypair.name;
        let sk = keypair.secret;
        let node_id = (pk,id);
        
        let (breeze_share_sender, breeze_share_receiver) =
            channel::<BreezeMessage>(CHANNEL_CAPACITY);
        let (breeze_confirm_sender, breeze_confirm_receiver) =
            channel::<BreezeMessage>(CHANNEL_CAPACITY);
        let (breeze_merkle_roots_sender, breeze_merkle_roots_receiver) =
            channel::<BreezeMessage>(CHANNEL_CAPACITY);
        let (breeze_reconstruct_secret_sender, breeze_reconstruct_secret_receiver) =
            channel::<BreezeMessage>(CHANNEL_CAPACITY);
        
        
        let valid_shares =Arc::new(RwLock::new(HashMap::new()));
        
        let my_dealer_shares: Arc<RwLock<HashMap<Epoch,Digest>>> = Arc::new(RwLock::new(HashMap::new()));
        let merkle_roots_received: Arc<RwLock<HashMap<Epoch,HashMap<PublicKey,Vec<Digest>>>>> = Arc::new(RwLock::new(HashMap::new()));
        let (merkle_watch_sender, merkle_watch_receiver) = watch::channel(());
        
        let common_reference_string = Arc::new(PQCrs::from(&common_reference_string));
        
        NetworkReceiver::spawn(
            address,
            BreezeMessageHandler {
                breeze_share_sender,
                breeze_confirm_sender,
                breeze_merkle_roots_sender,
                breeze_reconstruct_secret_sender
            },
        );

        let (breeze_recon_certificate_sender, breeze_recon_certificate_receiver) =
            channel::<(HashSet<Digest>,Epoch, usize)>(CHANNEL_CAPACITY);
        //lagrange interpolation to get result
        BreezeResult::spawn(
            // Arc::clone(&committee),
            committee.clone(),
            breeze_recon_certificate_receiver,
            breeze_reconstruct_secret_receiver,
            Arc::clone(&merkle_roots_received),
            merkle_watch_receiver,
            breeze_result_sender,
            Arc::clone(&common_reference_string),
        );

        //reconstruct phase
        BreezeReconstruct::spawn(
            node_id,
            // Arc::clone(&committee),
            committee.clone(),
            breeze_reconstruct_cmd_receiver,
            breeze_recon_certificate_sender,
            ReliableSender::new(),
            Arc::clone(&valid_shares),
            Arc::clone(&common_reference_string),
        );
        let committee = Arc::new(RwLock::new(committee));
        //confirm phase
        BreezeConfirm::spawn(
            node_id,
            Arc::clone(&committee),
            breeze_confirm_receiver,
            breeze_certificate_sender,
            Arc::clone(&my_dealer_shares),
        );
        //reply phase
        BreezeReply::spawn(
            node_id,
            sk,
            Arc::clone(&committee),
            breeze_share_receiver,
            breeze_merkle_roots_receiver,
            merkle_roots_received,
            merkle_watch_sender,
            ReliableSender::new(),
            Arc::clone(&valid_shares),
            Arc::clone(&common_reference_string),
        );

        //share phase
        BreezeShare::spawn(
            node_id,
            Arc::clone(&committee),
            breeze_share_cmd_receiver,
            ReliableSender::new(),
            Arc::clone(&common_reference_string),
            Arc::clone(&my_dealer_shares)
        );
    }
}
