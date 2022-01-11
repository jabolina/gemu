use crate::algorithm;
use crate::algorithm::handler::GenericMulticastReceiver;
use crate::algorithm::message::{Message, MessageType};
use crate::algorithm::AlgorithmError;
use crate::transport::group_communication::GroupCommunication;
use crate::transport::primitive::Transport;
use crate::transport::process_communication;
use crate::transport::process_communication::ProcessCommunication;
use crate::transport::Sender;
use crate::transport::{group_communication, TransportError};
use abro::TransportConfiguration;
use std::borrow::BorrowMut;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{JoinError, JoinHandle};

/// The Generic Multicast algorithm which is exposed for users.
///
/// Using this structure users can generic multicast a message to set of processes. This method
/// returns right away, it does not work as a request/response model, so, eventually the message
/// will be delivered.
pub struct GenericMulticast {
    // Used to notify the manager to publish a message.
    publish: mpsc::Sender<Message>,

    // The manager handle, used to stop the running thread.
    receiver_handle: JoinHandle<()>,

    // The algorithm handle, used to stop the running thread.
    sender_handle: JoinHandle<()>,

    // Used to notify that other threads to stop.
    shutdown_tx: broadcast::Sender<()>,
}

/// This structure is responsible for managing the transport primitives.
///
/// This will be used for the sending part of the algorithm. No messages will be received through
/// this manager. This manager will listen a [`Receiver`] so message that should be sent by the
/// correct communication primitive.
///
/// [`Receiver`]: mpsc::Receiver
struct TransportManager {
    // Messages that should be sent using one of the underlying primitives.
    message_rx: mpsc::Receiver<Message>,

    // The process communication primitive.
    process_transport: Transport<ProcessCommunication>,

    // The group communication primitive.
    group_transport: Transport<GroupCommunication>,
}

impl GenericMulticast {
    /// Build the [`GenericMulticast`] structure.
    ///
    /// This must receive the configuration for the underlying transport primitives that will be
    /// created. The complete process will start some threads to handle sending and receiving
    /// messages, all of threads will be closed by the structure when closed.
    ///
    /// Using this structure the user should be able to generic multicast a message to a set of
    /// processes and is the only exposed API for this interaction.
    ///
    /// # Example
    ///
    /// This structure will be created following the configuration given as arguments.
    ///
    /// ```no_run
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let configuration = abro::TransportConfiguration::builder()
    ///         .with_host("localhost")
    ///         .with_port(2379)
    ///         .with_partition("partition-name")
    ///         .build()
    ///         .expect("Should be configured");
    ///
    ///     let mut algorithm = gemu::algorithm::GenericMulticast::build(configuration).await
    ///         .expect("Should create algorithm correctly");
    ///
    ///     algorithm.gm_cast("message content", vec!["partition-name"]).await;
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// This method can fail if is not possible to correctly create the underlying primitives.
    pub async fn build(configuration: TransportConfiguration) -> algorithm::AlgorithmResult<Self> {
        let (message_tx, message_rx) = mpsc::channel(1024);
        let (publish_tx, publish_rx) = mpsc::channel(1024);
        let (shutdown_tx, _) = broadcast::channel(1);

        // First create the receiver, which is the algorithm implementation itself. The receiver
        // receive both channels to send and receive messages from the underlying primitives.
        let mut receiver = GenericMulticastReceiver::new(message_rx, publish_tx.clone());

        // Create the manager structure. This structure will instantiate the primitives and will
        // only receive messages. Using the channels to receive any message that should be sent.
        let mut writer =
            TransportManager::new(8080, configuration, publish_rx, message_tx.clone()).await?;

        // Now each of the structure must handle itself. The receiver is spawned to receive message
        // and execute the algorithm itself. The sender is spawned so it can only send messages.
        let receiver_subscriber = shutdown_tx.subscribe();
        let receiver_handle = tokio::spawn(async move { receiver.poll(receiver_subscriber).await });
        let sender_subscriber = shutdown_tx.subscribe();
        let sender_handle = tokio::spawn(async move { writer.poll(sender_subscriber).await });

        Ok(GenericMulticast {
            publish: publish_tx,
            receiver_handle,
            sender_handle,
            shutdown_tx,
        })
    }

    /// Generic multicast a message to the given destination.
    ///
    /// This method is used to generic multicast the given content to the given destinations. The
    /// message content must be serializable so it can be written to the sockets. The destination
    /// will be verified against an oracle implementation, than can transform the partition name
    /// back to the socket address.
    ///
    /// This method usually will not block. Meaning that this is not a request/response model, the
    /// message will eventually be delivered by the protocol.
    ///
    /// # Errors
    ///
    /// This method will asynchronously publish the message into a channel. This method can return
    /// an error if is not possible to write the message.
    pub async fn gm_cast(
        &mut self,
        content: impl Into<String>,
        destination: Vec<&str>,
    ) -> algorithm::AlgorithmResult<()> {
        let message = Message::build(&content.into(), destination);
        Ok(self.publish.send(message).await?)
    }

    /// This method will stop all running threads and consume the structure.
    ///
    /// This should be only when the [`GenericMulticast`] structure will not be used anymore. This
    /// method will stop all running threads, first it will emit a shutdown signal and then wait
    /// for the threads to stop, so is possible that it take some time to complete.
    ///
    /// # Errors
    ///
    /// This method panic if one of the underlying threads panicked.
    pub async fn stop(mut self) -> algorithm::AlgorithmResult<()> {
        drop(self.shutdown_tx);
        self.receiver_handle.borrow_mut().await?;
        self.sender_handle.borrow_mut().await?;
        Ok(())
    }
}

impl TransportManager {
    /// Creates a [`TransportManager`] structure.
    ///
    /// This will instantiate the underlying transport primitives using the given configuration.
    /// The channel given as arguments will be used to poll for incoming messages that should be
    /// published using the underlying primitives.
    ///
    /// # Errors
    ///
    /// This method can return an error if is not possible to create the underlying primitives.
    async fn new(
        port: usize,
        configuration: TransportConfiguration,
        consumer: mpsc::Receiver<Message>,
        producer: mpsc::Sender<String>,
    ) -> algorithm::AlgorithmResult<Self> {
        let group_transport = group_communication::new(configuration, producer.clone()).await?;
        let process_transport = process_communication::new(port, producer.clone()).await?;

        Ok(TransportManager {
            message_rx: consumer,
            group_transport,
            process_transport,
        })
    }

    /// Poll for sending messages.
    ///
    /// This method will run until a signal is received in the channel or if the incoming channel
    /// is closed. This should be called while spawning a new thread, since this will keep polling.
    async fn poll(&mut self, mut shutdown_rx: broadcast::Receiver<()>) {
        // We must be cancel safe, meaning we cannot partially send a message to only a subset of
        // the destination. If this future is cancelled, we must complete sending the messages and
        // only then return.
        let send_message = async move {
            // While the incoming channel is open, there is messages to send.
            while let Some(message) = self.message_rx.recv().await {
                let destinations = message.destination().clone();

                // Different types of messages can be sent, so we used this convenience method to
                // identify which type of primitive should be used.
                match message.r#type() {
                    // A message that should be sent directly to a process, so we need to know the
                    // actual socket address, so we use the oracle implementation provided by the
                    // user so we can translate a partition to the socket addresses of all the
                    // processes within a given partition.
                    MessageType::Process => {
                        let content: String = Message::into(message);
                        for destination in destinations {
                            self.process_transport.send(&destination, &content).await;
                        }
                    }

                    // A broadcast message will broadcast to a specific group of processes, thus,
                    // there is no need to translate the partition name.
                    MessageType::Broadcast => {
                        let content: String = Message::into(message);
                        for destination in destinations {
                            self.group_transport.send(&destination, &content).await;
                        }
                    }
                };
            }
        };

        tokio::select! {
            _ = send_message => {},

            // This is the supposed branch that should be selected.
            _ = shutdown_rx.recv() => {}
        }
    }
}

/// Transform a [`TransportError`] to an [`AlgorithmError`].
impl From<TransportError> for AlgorithmError {
    fn from(_: TransportError) -> Self {
        todo!()
    }
}

/// Transform a [`SendError`] to the [`AlgorithmError`].
///
/// This occur when is not possible to write a Tokio channel.
///
/// [`SendError`]: tokio::sync::mpsc::error::SendError
impl From<mpsc::error::SendError<Message>> for AlgorithmError {
    fn from(_: mpsc::error::SendError<Message>) -> Self {
        todo!()
    }
}

/// Transforms the [`JoinError`] to the [`AlgorithmError`]
///
/// This can occur during the shutdown, when the underlying thread panicked.
impl From<JoinError> for AlgorithmError {
    fn from(_: JoinError) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithm::message::Message;
    use crate::algorithm::writer::{GenericMulticast, TransportManager};
    use abro::TransportConfiguration;
    use tokio::sync::{broadcast, mpsc};

    #[ignore]
    #[tokio::test]
    async fn should_send_messages() {
        let configuration = TransportConfiguration::builder()
            .with_partition("partition")
            .with_port(2379)
            .with_host("localhost")
            .build();
        assert!(configuration.is_ok());

        let (shutdown_tx, _) = broadcast::channel(1);
        let (message_tx, mut message_rx) = mpsc::channel(1024);
        let (publish_tx, publish_rx) = mpsc::channel(1024);
        let manager =
            TransportManager::new(0, configuration.unwrap(), publish_rx, message_tx.clone()).await;

        assert!(manager.is_ok());
        let shutdown = shutdown_tx.subscribe();
        let sender_handle = tokio::spawn(async move { manager.unwrap().poll(shutdown).await });

        let mut algorithm = GenericMulticast {
            publish: publish_tx,
            receiver_handle: tokio::spawn(async {}),
            sender_handle,
            shutdown_tx,
        };

        let response = algorithm.gm_cast("hello!", vec!["partition"]).await;
        assert!(response.is_ok());

        let message = tokio::time::timeout(std::time::Duration::from_secs(2), async move {
            message_rx.recv().await
        })
        .await;
        assert!(message.is_ok());

        let message = message.unwrap();
        assert!(message.is_some());

        let message = Message::from(message.unwrap());
        assert_eq!(message.destination().len(), 1);

        let shutdown = algorithm.stop().await;
        assert!(shutdown.is_ok());
    }
}
