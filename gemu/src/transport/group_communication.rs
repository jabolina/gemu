use crate::transport;
use crate::transport::{AsyncTrait, Sender, TransportError};
use abro::Error;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

/// Implements the group communication primitive. The underlying group communication is atomic
/// broadcast.
pub(crate) struct GroupCommunication {
    // The transmitter that wraps all the atomic broadcast complexities.
    communication_tx: abro::Sender,
}

/// Creates a new [`Transport`] of type [`GroupCommunication`].
///
/// The atomic broadcast and the listener are received as arguments and new [`Transport`] primitive
/// for group communication is created.
///
/// # Errors
///
/// This can return an error if is not possible to create the atomic broadcast primitive. This
/// probably means that is not possible to connect to the etcd server.
///
/// [`Transport`]: transport::Transport
pub(crate) async fn new(
    configuration: abro::TransportConfiguration,
    producer: mpsc::Sender<String>,
) -> transport::TransportResult<transport::Transport<GroupCommunication>> {
    let (communication_tx, communication_rx) = abro::channel(configuration).await?;
    let receive = || async move {
        let _ = communication_rx
            .listen(|data| async {
                if let Ok(data) = data {
                    let _ = producer.send(data).await;
                }
                Ok(true)
            })
            .await;
    };
    let primitive = GroupCommunication { communication_tx };
    Ok(transport::Transport::new(primitive, receive))
}

/// Implements the [`AsyncTrait`] for the current [`GroupCommunication`].
///
/// This is the standard return using a pinned box [`Future`], the [`Future`] must be [`Send`] so
/// is possible to send between threads.
impl<'a> AsyncTrait<'a, transport::TransportResult<()>> for GroupCommunication {
    type Future = Pin<Box<dyn Future<Output = transport::TransportResult<()>> + Send + 'a>>;
}

impl<'a> Sender<'a> for GroupCommunication {
    /// Broadcast a message to a group of processes.
    ///
    /// This will broadcast the message atomically for the processes within the given destination.
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future {
        Box::pin(async move {
            let message = abro::Message::new(destination, data);
            Ok(self.communication_tx.send(message).await?)
        })
    }
}

/// Transforms an [`abro::Error`] into a [`TransportError`].
impl From<abro::Error> for TransportError {
    fn from(e: Error) -> Self {
        TransportError::SetupError(format!("{:?}", e))
    }
}
