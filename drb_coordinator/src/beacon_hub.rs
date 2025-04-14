use crate::error::DrbError;
use log::info;
use model::types_and_const::{Epoch, RandomNum, BEACON_PER_EPOCH};
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::sleep;

pub struct BeaconHub;

impl BeaconHub {
    pub fn spawn(
        beacon_recon_req_sender: Sender<(Epoch, usize)>,
        mut beacon_res_receiver: Receiver<((Epoch, usize), Result<RandomNum, DrbError>)>,
    ) {
        let beacon_per_epoch = *BEACON_PER_EPOCH.get().unwrap();
        // tokio::spawn(async move {
        //     let mut current_epoch = 0;
        //     loop {
        //         for index in 1..=beacon_per_epoch {
        //             sleep(Duration::from_millis(20)).await;
        //             beacon_recon_req_sender
        //                 .send((current_epoch, index as usize))
        //                 .await
        //                 .unwrap()
        //         }
        //         let mut has_common_core = true;
        //         for _ in 1..=beacon_per_epoch {
        //             match beacon_res_receiver.recv().await.unwrap() {
        //                 ((e, i), Ok(random)) => {
        //                     info!("Beacon output for epoch:{} index:{} is {}", e, i, random);
        //                 }
        //                 (_, Err(_)) => {
        //                     has_common_core = false;
        //                 }
        //             }
        //         }
        //         if !has_common_core {
        //             sleep(Duration::from_millis(500)).await;
        //         } else {
        //             current_epoch += 1;
        //         }
        //     }
        // });

        tokio::spawn(async move {
            let mut current_epoch = 0;
            let mut current_index = 1;
            loop {
                sleep(Duration::from_millis(50)).await;
                beacon_recon_req_sender
                    .send((current_epoch, current_index as usize))
                    .await
                    .unwrap();
                match beacon_res_receiver.recv().await.unwrap() {
                    ((e, i), Ok(random)) => {
                        info!("Beacon output for epoch:{} index:{} is {}", e, i, random);
                        if current_index < beacon_per_epoch {
                            current_index += 1;
                        }else {
                            current_epoch += 1;
                            current_index = 1;
                        }
                    }
                    (_, Err(_)) => {
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });
    }
}
