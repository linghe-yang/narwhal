// Copyright(C) Facebook, Inc. and its affiliates.
use crate::error::NetworkError;
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::SplitSink;
use futures::stream::StreamExt as _;
use log::{debug, info, warn};
use std::error::Error;
use std::io::Read;
use std::net::SocketAddr;
use flate2::read::ZlibDecoder;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use model::types_and_const::MAX_FRAME_SIZE;

#[cfg(test)]
#[path = "tests/receiver_tests.rs"]
pub mod receiver_tests;

/// Convenient alias for the writer end of the TCP channel.
pub type Writer = SplitSink<Framed<TcpStream, LengthDelimitedCodec>, Bytes>;

#[async_trait]
pub trait MessageHandler: Clone + Send + Sync + 'static {
    /// Defines how to handle an incoming message. A typical usage is to define a `MessageHandler` with a
    /// number of `Sender<T>` channels. Then implement `dispatch` to deserialize incoming messages and
    /// forward them through the appropriate delivery channel. Then `writer` can be used to send back
    /// responses or acknowledgements to the sender machine (see unit tests for examples).
    async fn dispatch(&self, writer: &mut Writer, message: Bytes) -> Result<(), Box<dyn Error>>;
}

/// For each incoming request, we spawn a new runner responsible to receive messages and forward them
/// through the provided deliver channel.
pub struct Receiver<Handler: MessageHandler> {
    /// Address to listen to.
    address: SocketAddr,
    /// Struct responsible to define how to handle received messages.
    handler: Handler,
}

impl<Handler: MessageHandler> Receiver<Handler> {
    /// Spawn a new network receiver handling connections from any incoming peer.
    pub fn spawn(address: SocketAddr, handler: Handler) {
        tokio::spawn(async move {
            Self { address, handler }.run().await;
        });
    }

    /// Main loop responsible to accept incoming connections and spawn a new runner to handle it.
    async fn run(&self) {
        let listener = TcpListener::bind(&self.address)
            .await
            .expect("Failed to bind TCP port");

        debug!("Listening on {}", self.address);
        loop {
            let (socket, peer) = match listener.accept().await {
                Ok(value) => value,
                Err(e) => {
                    warn!("{}", NetworkError::FailedToListen(e));
                    continue;
                }
            };
            info!("Incoming connection established with {}", peer);
            Self::spawn_runner(socket, peer, self.handler.clone()).await;
        }
    }

    /// Spawn a new runner to handle a specific TCP connection. It receives messages and process them
    /// using the provided handler.
    // async fn spawn_runner(socket: TcpStream, peer: SocketAddr, handler: Handler) {
    //     tokio::spawn(async move {
    //         let codec = LengthDelimitedCodec::builder()
    //             .max_frame_length(MAX_FRAME_SIZE)
    //             .new_codec();
    //         let transport = Framed::new(socket, codec);
    //         let (mut writer, mut reader) = transport.split();
    //         while let Some(frame) = reader.next().await {
    //             match frame.map_err(|e| NetworkError::FailedToReceiveMessage(peer, e)) {
    //                 Ok(message) => {
    //                     if let Err(e) = handler.dispatch(&mut writer, message.freeze()).await {
    //                         warn!("{}", e);
    //                         return;
    //                     }
    //                 }
    //                 Err(e) => {
    //                     warn!("{}", e);
    //                     return;
    //                 }
    //             }
    //         }
    //         warn!("Connection closed by peer {}", peer);
    //     });
    // }
    async fn spawn_runner(socket: TcpStream, peer: SocketAddr, handler: Handler) {
        tokio::spawn(async move {
            let codec = LengthDelimitedCodec::builder()
                .max_frame_length(MAX_FRAME_SIZE)
                .new_codec();
            let transport = Framed::new(socket, codec);
            let (mut writer, mut reader) = transport.split();
            while let Some(frame) = reader.next().await {
                match frame.map_err(|e| NetworkError::FailedToReceiveMessage(peer, e)) {
                    Ok(message) => {
                        let message = message.freeze();
                        // 检查标志位
                        if message.is_empty() {
                            warn!("Received empty message from {}", peer);
                            continue;
                        }
                        let flag = message[0];
                        let payload = &message[1..];
                        let processed_message = match flag {
                            0x00 => payload.to_vec().into(),
                            0x01 => {
                                let mut decoder = ZlibDecoder::new(payload);
                                let mut decompressed = Vec::new();
                                if let Err(e) = decoder.read_to_end(&mut decompressed) {
                                    warn!("Failed to decompress message from {}: {}", peer, e);
                                    continue;
                                }
                                decompressed.into()
                            }
                            _ => {
                                warn!("Invalid compression flag {} from {}", flag, peer);
                                continue;
                            }
                        };
                        if let Err(e) = handler.dispatch(&mut writer, processed_message).await {
                            warn!("{}", e);
                            return;
                        }
                    }
                    Err(e) => {
                        warn!("{}", e);
                        return;
                    }
                }
            }
            warn!("Connection closed by peer {}", peer);
        });
    }
}
