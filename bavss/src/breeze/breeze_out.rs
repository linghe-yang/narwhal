// 
// use std::collections::HashMap;
// use std::sync::Arc;
// use log::{info};
// use tokio::sync::mpsc::{Receiver, Sender};
// use tokio::sync::RwLock;
// use config::Committee;
// use model::breeze_structs::{BreezeCertificate, BreezeContent, BreezeMessage};
// use model::scale_type::{Epoch, Id};
// 
// pub struct BreezeOut {
//     node_id: Id,
//     committee: Arc<RwLock<Committee>>,
//     breeze_out_receiver: Receiver<BreezeMessage>,
//     breeze_s_e_set_sender: Sender<(Epoch, Vec<BreezeCertificate>)>,
// }
// 
// impl BreezeOut {
//     pub fn spawn(
//         node_id: Id,
//         committee: Arc<RwLock<Committee>>,
//         breeze_out_receiver: Receiver<BreezeMessage>,
//         breeze_s_e_set_sender: Sender<(Epoch, Vec<BreezeCertificate>)>,
//     ) {
//         tokio::spawn(async move {
//             Self {
//                 node_id,
//                 committee,
//                 breeze_out_receiver,
//                 breeze_s_e_set_sender,
//             }
//                 .run()
//                 .await;
//         });
//     }
// 
//     pub async fn run(&mut self) {
//         info!("Starting Breeze Output");
//         let mut s_e_set: HashMap<Epoch, Vec<BreezeCertificate>> = HashMap::new();
//         let mut delivered_s_e_set: Vec<Epoch> = Vec::new();
//         loop {
//             match self.breeze_out_receiver.recv().await.unwrap() {
//                 message => {
//                     let epoch = match message.get_epoch() {
//                         Some(epoch) => epoch,
//                         None => {
//                             continue;
//                         }
//                     };
//                     if delivered_s_e_set.contains(&epoch) {
//                         continue;
//                     }
//                     let cer;
//                     if let BreezeContent::Confirm(rm) = message.content {
//                         cer = rm.cer;
//                     } else {
//                         continue;
//                     }
//                     let committee = self.committee.read().await;
//                     let quorum_threshold = committee.authorities_quorum_threshold();
//                     if !cer.verify(quorum_threshold){
//                         continue;
//                     }
//                     s_e_set.entry(epoch)
//                         .or_insert_with(Vec::new)
//                         .push(cer);
//                     let s_e_set_threshold = committee.authorities_fault_tolerance() + 1;
//                     let mut keys_to_remove = Vec::new();
//                     for (e,set) in s_e_set.iter(){
//                         if set.len() >= s_e_set_threshold && !delivered_s_e_set.contains(&e){
//                             self.breeze_s_e_set_sender.send((*e, set.clone())).await.expect("Failed to send breeze output Se set");
//                             keys_to_remove.push(*e);
//                             delivered_s_e_set.push(*e);
//                         }
//                     }
//                     for key in keys_to_remove {
//                         s_e_set.retain(|&e, _| e > key);
//                     }
//                     // if s_e_set.len() >= s_e_set_threshold && !delivered_s_e_set.contains(&epoch) {
//                     //     warn!("I can send se set");
//                     //     let s_e_set_to_send = s_e_set.remove(&epoch).unwrap();
//                     //     self.breeze_s_e_set_sender.send((epoch,s_e_set_to_send)).await.expect("Failed to send breeze output Se set");
//                     // 
//                     //     delivered_s_e_set.push(epoch);
//                     // }
// 
//                 }
//             }
//         }
//     }
// }
