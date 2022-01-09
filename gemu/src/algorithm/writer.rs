use crate::algorithm::message::{Message, MessageType};
use crate::transport::group_communication;
use crate::transport::group_communication::GroupCommunication;
use crate::transport::primitive::Transport;
use crate::transport::process_communication;
use crate::transport::process_communication::ProcessCommunication;
use crate::transport::Sender;
use crate::Error;
use abro::TransportConfiguration;
use tokio::sync::{broadcast, mpsc};

pub(crate) struct TransportHandler {
    message_rx: mpsc::Receiver<Message>,
    process_transport: Transport<ProcessCommunication>,
    group_transport: Transport<GroupCommunication>,
}

impl TransportHandler {
    pub(crate) async fn new(
        port: usize,
        configuration: TransportConfiguration,
        consumer: mpsc::Receiver<Message>,
        producer: mpsc::Sender<String>,
    ) -> Result<Self, Error> {
        let group_transport = group_communication::new(configuration, producer.clone()).await?;
        let process_transport = process_communication::new(port, producer.clone()).await?;
        Ok(TransportHandler {
            message_rx: consumer,
            group_transport,
            process_transport,
        })
    }
}

pub(crate) async fn send_messages(
    mut handler: TransportHandler,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let send_message = async move {
        while let Some(message) = handler.message_rx.recv().await {
            match message.r#type() {
                MessageType::Process => {
                    for destination in message.destination() {
                        handler
                            .process_transport
                            .send(destination, message.content())
                            .await;
                    }
                }
                MessageType::Broadcast => {
                    for destination in message.destination() {
                        handler
                            .group_transport
                            .send(destination, message.content())
                            .await;
                    }
                }
            };
        }
    };

    tokio::select! {
        _ = send_message => {},
        _ = shutdown_rx.recv() => {}
    }
}
