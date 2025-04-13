use crate::error::DrbError;
use config::Committee;
use model::breeze_universal::{BreezeCertificate, BreezeReconRequest};
use model::types_and_const::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use log::info;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;

pub struct Coordinator {
    committee: Arc<RwLock<Committee>>,
    // share phase
    b_share_cmd_sender: Sender<Epoch>,
    certificate_from_breeze: Receiver<BreezeCertificate>,
    // share reply confirm end : certificate received
    // propose certificate to the consensus
    certificate_to_consensus: Sender<BreezeCertificate>,
    certificate_to_init_consensus: Sender<BreezeCertificate>,
    cer_decided_from_consensus: Receiver<BreezeCertificate>,
    cc_decided_from_init_consensus: Receiver<HashSet<BreezeCertificate>>,
    // common core get
    // recon request from consensus
    global_coin_recon_req_receiver: Receiver<Round>,
    // recon request from beacon consumer
    beacon_recon_req_receiver: Receiver<(Epoch, usize)>,
    // relay recon request to breeze
    b_recon_req_sender: Sender<BreezeReconRequest>,
    b_recon_res_receiver: Receiver<(Epoch, usize, RandomNum)>,
    // send reconstructed global random coin to consensus
    global_coin_res_sender: Sender<(Round, Result<RandomNum, DrbError>)>,
    // send reconstructed beacon value to consumer
    beacon_res_sender: Sender<((Epoch, usize),Result<RandomNum, DrbError>)>,

    certificate_buffer: HashMap<Epoch, HashSet<BreezeCertificate>>,

    decided_common_core: HashSet<Epoch>,
    beacon_reconstructed: HashMap<(Epoch, usize), RandomNum>,
}

impl Coordinator {
    pub async fn spawn(
        // committee: Arc<RwLock<Committee>>,
        committee: Committee,
        b_share_cmd_sender: Sender<Epoch>,
        certificate_from_breeze: Receiver<BreezeCertificate>,
        certificate_to_consensus: Sender<BreezeCertificate>,
        certificate_to_init_consensus: Sender<BreezeCertificate>,
        cer_decided_from_consensus: Receiver<BreezeCertificate>,
        cc_decided_from_init_consensus: Receiver<HashSet<BreezeCertificate>>,
        global_coin_recon_req_receiver: Receiver<Round>,
        beacon_recon_req_receiver: Receiver<(Epoch, usize)>,
        b_recon_req_sender: Sender<BreezeReconRequest>,
        b_recon_res_receiver: Receiver<(Epoch, usize, RandomNum)>,
        global_coin_res_sender: Sender<(Round, Result<RandomNum, DrbError>)>,
        beacon_res_sender: Sender<((Epoch, usize),Result<RandomNum, DrbError>)>,
    ) {
        let certificate_buffer = HashMap::new();
        let decided_common_core = HashSet::new();
        let beacon_reconstructed = HashMap::new();
        let committee = Arc::new(RwLock::new(committee));
        tokio::spawn(async move {
            Self {
                committee,
                b_share_cmd_sender,
                certificate_from_breeze,
                certificate_to_consensus,
                certificate_to_init_consensus,
                cer_decided_from_consensus,
                cc_decided_from_init_consensus,
                global_coin_recon_req_receiver,
                beacon_recon_req_receiver,
                b_recon_req_sender,
                b_recon_res_receiver,
                global_coin_res_sender,
                beacon_res_sender,

                certificate_buffer,
                decided_common_core,
                beacon_reconstructed,
            }
            .run()
            .await;
        });
    }
    async fn run(&mut self) {
        self.b_share_cmd_sender.send(0).await.unwrap();
        let max_epoch = *MAX_EPOCH.get().unwrap();
        let beacon_per_epoch = *BEACON_PER_EPOCH.get().unwrap();
        loop {
            tokio::select! {
                Some(cer) = self.certificate_from_breeze.recv() => {
                    if cer.epoch == 0{
                        self.certificate_to_init_consensus.send(cer).await.unwrap();
                    }else {
                        self.certificate_to_consensus.send(cer).await.unwrap();
                    }
                }
                Some(cc) = self.cc_decided_from_init_consensus.recv()=>{
                    self.certificate_buffer.insert(0, cc);
                    self.decided_common_core.insert(0);
                    self.b_share_cmd_sender.send(1).await.unwrap();
                }
                Some(cer) = self.cer_decided_from_consensus.recv() =>{
                    let epoch = cer.epoch;
                    if self.decided_common_core.contains(&epoch) {
                        continue;
                    }
                    let inner_map = self.certificate_buffer
                        .entry(epoch)
                        .or_insert_with(HashSet::new);
                    inner_map.insert(cer);
                    let committee = self.committee.read().await;
                    let fault_tolerance = committee.authorities_fault_tolerance();
                    if inner_map.len() >= fault_tolerance + 1{
                        self.decided_common_core.insert(epoch);
                        self.b_share_cmd_sender.send(epoch + 1).await.unwrap();
                    }
                }

                Some(round) = self.global_coin_recon_req_receiver.recv() =>{
                    let (mut epoch, index) = dolphin_round_to_epoch_index(round, max_epoch);
                    if index > max_epoch as usize {
                        self.global_coin_res_sender.send((round,Err(DrbError::InvalidIndex))).await.unwrap();
                        continue;
                    }
                    epoch -= 1;
                    if !self.decided_common_core.contains(&epoch){
                        self.global_coin_res_sender.send((round,Err(DrbError::NoCommonCore))).await.unwrap();
                        continue;
                    }
                    let mut flag = true;
                    if let Some(v) = self.beacon_reconstructed.get(&(epoch,index)){
                        self.global_coin_res_sender.send((round,Ok(*v))).await.unwrap();
                        flag = false;
                    }
                    if flag{
                        let certificates = &self.certificate_buffer[&epoch];
                        let recon_req = BreezeReconRequest{
                            c: certificates.iter().map(|x| x.c).collect(),
                            epoch,
                            index
                        };
                        self.b_recon_req_sender.send(recon_req).await.unwrap();
                    }
                }

                Some((epoch,index)) = self.beacon_recon_req_receiver.recv() =>{
                    if !self.decided_common_core.contains(&epoch){
                        self.beacon_res_sender.send(((epoch,index),Err(DrbError::NoCommonCore))).await.unwrap();
                        continue;
                    }
                    if index > beacon_per_epoch as usize{
                        self.beacon_res_sender.send(((epoch,index),Err(DrbError::InvalidIndex))).await.unwrap();
                        continue;
                    }
                    let mut flag = true;
                    if let Some(v) = self.beacon_reconstructed.get(&(epoch,index)){
                        self.beacon_res_sender.send(((epoch,index),Ok(*v))).await.unwrap();
                        flag = false;
                    }
                    if flag{
                        let certificates = &self.certificate_buffer[&epoch];
                        let recon_req = BreezeReconRequest{
                            c: certificates.iter().map(|x| x.c).collect(),
                            epoch,
                            index:index + max_epoch as usize
                        };
                        self.b_recon_req_sender.send(recon_req).await.unwrap();
                    }
                }

                Some((epoch,index,value)) = self.b_recon_res_receiver.recv() =>{
                    self.beacon_reconstructed.insert((epoch,index),value);
                    if index <= max_epoch as usize{
                        let round = dolphin_epoch_index_to_round(epoch +1,index,max_epoch);
                        self.global_coin_res_sender.send((round,Ok(value))).await.unwrap();
                    } else if index <= (max_epoch+ beacon_per_epoch) as usize{
                        self.beacon_res_sender.send(((epoch,index - max_epoch as usize),Ok(value))).await.unwrap();
                    }
                }

            }
        }
    }
}
