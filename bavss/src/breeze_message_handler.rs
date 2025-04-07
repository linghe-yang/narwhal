use std::error::Error;
use async_trait::async_trait;
use bytes::Bytes;
use futures::SinkExt;
use tokio::sync::mpsc::{Sender};
use model::breeze_structs::{BreezeContent, BreezeMessage};
use network::{MessageHandler, Writer};

#[derive(Clone)]
pub struct BreezeMessageHandler {
    pub breeze_share_sender: Sender<BreezeMessage>,
    pub breeze_confirm_sender: Sender<BreezeMessage>,
    pub breeze_out_sender: Sender<BreezeMessage>,
    pub breeze_reconstruct_secret_sender: Sender<BreezeMessage>
}

#[async_trait]
impl MessageHandler for BreezeMessageHandler {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        let _ = writer.send(Bytes::from("Ack")).await;

        let message: BreezeMessage = bincode::deserialize(&serialized).unwrap();

        match message.content {
            BreezeContent::Share(_) => {
                self.breeze_share_sender
                    .send(message)
                    .await
                    .expect("Failed to send secret to breeze validator");
            }
            BreezeContent::Reply(_) => {
                self.breeze_confirm_sender
                    .send(message)
                    .await
                    .expect("Failed to send reply to breeze confirm phase");
            }
            BreezeContent::Confirm(_) => {
                self.breeze_out_sender
                    .send(message)
                    .await
                    .expect("Failed to send reply to breeze confirm phase");
            }
            BreezeContent::Reconstruct(_) => {
                self.breeze_reconstruct_secret_sender
                    .send(message)
                    .await
                    .expect("Failed to send reply to breeze reconstruct phase");
            }
        }
        Ok(())
    }
}