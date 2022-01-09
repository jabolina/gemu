use crate::algorithm::handler::NextStep::NoOp;
use crate::algorithm::message::{Message, MessageType};
use crate::algorithm::writer::{send_messages, TransportHandler};
use crate::algorithm::AlgorithmError;
use crate::Error;
use abro::TransportConfiguration;
use std::borrow::BorrowMut;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{JoinError, JoinHandle};

struct GenericMulticastReceiver {
    consumer: mpsc::Receiver<String>,
    publish: mpsc::Sender<Message>,
}

struct GenericMulticastSender {
    publish: mpsc::Sender<Message>,
    receiver_handle: JoinHandle<()>,
    sender_handle: JoinHandle<()>,
    shutdown_tx: broadcast::Sender<()>,
}

enum NextStep {
    NoOp,
    SendProcesses,
    SendGroup,
}

impl GenericMulticastSender {
    async fn new(configuration: TransportConfiguration) -> Result<Self, Error> {
        let (message_tx, message_rx) = mpsc::channel(1024);
        let (publish_tx, publish_rx) = mpsc::channel(1024);
        let (shutdown_tx, _) = broadcast::channel(1);
        let receiver = GenericMulticastReceiver {
            consumer: message_rx,
            publish: publish_tx.clone(),
        };
        let writer =
            TransportHandler::new(8080, configuration, publish_rx, message_tx.clone()).await?;
        let receiver_subscriber = shutdown_tx.subscribe();
        let receiver_handle =
            tokio::spawn(async move { poll(receiver, receiver_subscriber).await });
        let sender_subscriber = shutdown_tx.subscribe();
        let sender_handle =
            tokio::spawn(async move { send_messages(writer, sender_subscriber).await });

        Ok(GenericMulticastSender {
            publish: publish_tx,
            receiver_handle,
            sender_handle,
            shutdown_tx,
        })
    }
}

impl GenericMulticastSender {
    async fn gm_cast(&mut self, message: Message) -> Result<(), Error> {
        Ok(self.publish.send(message).await?)
    }

    async fn stop(mut self) -> Result<(), Error> {
        drop(self.shutdown_tx);
        self.receiver_handle.borrow_mut().await?;
        self.sender_handle.borrow_mut().await?;
        Ok(())
    }
}

impl GenericMulticastReceiver {
    async fn receive_message(&mut self, data: String) {
        let message = Message::parse(data);
        let next = match message.r#type() {
            MessageType::Broadcast => self.compute_group_timestamp().await,
            MessageType::Process => self.gather_group_timestamps().await,
        };

        if let Ok(next) = next {
            match next {
                NextStep::SendGroup | NextStep::SendProcesses => self.publish.send(message).await,
                NextStep::NoOp => Ok(()),
            };
        }
    }

    async fn compute_group_timestamp(&mut self) -> AlgorithmResult {
        Ok(NoOp)
    }

    async fn gather_group_timestamps(&mut self) -> AlgorithmResult {
        Ok(NoOp)
    }

    async fn do_deliver(&self) {}
}

impl From<tokio::sync::mpsc::error::SendError<Message>> for Error {
    fn from(_: SendError<Message>) -> Self {
        todo!()
    }
}

impl From<JoinError> for Error {
    fn from(_: JoinError) -> Self {
        todo!()
    }
}

async fn poll(mut receiver: GenericMulticastReceiver, mut shutdown_rx: broadcast::Receiver<()>) {
    let handle_messages = async move {
        while let Some(data) = receiver.consumer.recv().await {
            // This should be cancel safe.
            receiver.receive_message(data).await;
        }
    };

    tokio::select! {
        _ = handle_messages => {},
        _ = shutdown_rx.recv() => {},
    }
}

type AlgorithmResult = std::result::Result<NextStep, AlgorithmError>;
