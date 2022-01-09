//! The basic process communication primitive.
//!
//! Here is implemented the simple process communication primitive, which is backed by the
//! [`mepa`] TCP implementation. We expose only a single `send` API, used to send a message to the
//! specific address and the serializable data.
//!
//! The creation will receive a [`Listener`] which is responsible to listen and handle messages. So
//! messages can be polled right after the transport creation.
//!
//! [`mepa`]: mepa::Transport
//! [`Listener`]: crate::transport::Listener

use crate::transport;
use crate::transport::{AsyncTrait, Sender, TransportError};
use std::future::Future;
use std::net::AddrParseError;
use std::pin::Pin;
use tokio::sync::mpsc;

use mepa::Error;

/// The process structure responsible for sending messages between processes.
pub(crate) struct ProcessCommunication {
    // The underlying TCP primitive implementation, we hold only the transmitter part, since we
    // only send messages using this structure.
    communication_tx: mepa::Sender,
}

/// Creates a new [`ProcessCommunication`], this will start polling new message right away.
///
/// This will try to bind to the given port and start polling for new messages. Each received
/// message will be handled by the given listener. Since we poll for messages on another task,
/// the [`Listener`] must have both [`Send`] and [`Sync`], as well the returned [`Future`].
///
/// # Errors
///
/// This will return an error if is not possible to create the underlying TCP connection. Some
/// of the possible errors is trying to use a door without permission, < 1023, for example.
pub(crate) async fn new(
    port: usize,
    producer: mpsc::Sender<String>,
) -> transport::TransportResult<transport::Transport<ProcessCommunication>> {
    let (communication_tx, mut communication_rx) = mepa::channel(port).await?;
    let receive = || async move {
        let _ = communication_rx
            .poll(|data| async {
                let _ = producer.send(data).await;
            })
            .await;
    };
    let primitive = ProcessCommunication { communication_tx };
    Ok(transport::Transport::new(primitive, receive))
}

/// Implements the [`AsyncTrait`] for the [`ProcessCommunication`].
impl<'a> AsyncTrait<'a, transport::TransportResult<()>> for ProcessCommunication {
    type Future = Pin<Box<dyn Future<Output = transport::TransportResult<()>> + Send + 'a>>;
}

impl<'a> Sender<'a> for ProcessCommunication {
    /// Send a the data to the destination.
    ///
    /// Will send the given data to the given destination using the underlying TCP connection. The
    /// data must be serializable in order to be written to the socket.
    ///
    /// # Errors
    ///
    /// This can fail for any error that may happen while handling the socket, for example, writing
    /// the data or establishing the connection.
    /// This method can also fail if the given destination could not be parsed to a [`SocketAddr`].
    ///
    /// [`SocketAddr`]: std::net::addr::SocketAddr
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future {
        Box::pin(async move {
            Ok(self
                .communication_tx
                .send(destination.parse()?, data)
                .await?)
        })
    }
}

/// Transform into a [`TransportError`]
impl From<mepa::Error> for TransportError {
    fn from(e: Error) -> Self {
        match e {
            mepa::Error::SocketError(v) => TransportError::SendingError(v),
            e => TransportError::SetupError(format!("{:?}", e)),
        }
    }
}

/// Transform a [`AddrParserError`] into a [`TransportError`].
///
/// This error happen only when sending a message, so the [`TransportError::WriteError`] is used.
impl From<AddrParseError> for TransportError {
    fn from(e: AddrParseError) -> Self {
        TransportError::SendingError(e.to_string())
    }
}
