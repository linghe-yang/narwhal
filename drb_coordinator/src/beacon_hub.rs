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
        beacon_req_delay: u64
    ) {
        let beacon_per_epoch = *BEACON_PER_EPOCH.get().unwrap();
        tokio::spawn(async move {
            let mut current_epoch = 0;
            let mut current_index = 1;
            loop {
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
                        sleep(Duration::from_millis(beacon_req_delay)).await;
                    }
                    (_, Err(_)) => {
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });
    }
}
