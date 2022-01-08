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
use crate::transport::{AsyncTrait, Listener, Sender, TransportError};
use std::future::Future;
use std::net::AddrParseError;
use std::pin::Pin;

use mepa::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::task::JoinError;

/// The process structure responsible for holding all necessary information to send messages and to
/// correctly shutdown the underlying primitive.
pub(crate) struct ProcessCommunication {
    // The underlying TCP primitive implementation, we hold only the transmitter part, since we
    // only send messages using this structure.
    communication_tx: mepa::Sender,
}

/// Function responsible to receive new messages from the underlying receiver.
///
/// This method should execute during the whole application lifetime, receiving messages until the
/// signal is received through the channel notifying about the shutdown.
async fn poll<'a, T: Listener<'a>>(
    listener: T,
    mut rx: mepa::Receiver,
    mut signal: mpsc::Receiver<()>,
) {
    let polling = async move {
        let _ = rx
            .poll(|data| async {
                let _ = listener.handle(data).await;
            })
            .await;
    };

    tokio::select! {
        // The polling method, in a happy way will execute forever without any errors.
        _ = polling => {}

        // In the happy way, this is the future which will be completed first. When a signal is
        // received it means that we must stop polling for messages, so we just exit the select
        // automatically canceling the polling method.
        _ = signal.recv() => {}
    };
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
pub(crate) async fn new<'a, T>(
    port: usize,
    listener: T,
) -> transport::TransportResult<transport::primitive::Transport<ProcessCommunication>>
where
    T: Listener<'a> + 'static,
{
    let create = |shutdown_rx| async move {
        let (communication_tx, communication_rx) = mepa::channel(port).await?;
        let poll_join = tokio::spawn(async move {
            poll(listener, communication_rx, shutdown_rx).await;
        });
        let primitive = ProcessCommunication { communication_tx };
        Ok((primitive, poll_join))
    };
    Ok(transport::primitive::Transport::new(create).await?)
}

impl<'a> AsyncTrait<'a> for ProcessCommunication {
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
        TransportError::WriteError(format!("{:?}", e))
    }
}

impl From<AddrParseError> for TransportError {
    fn from(e: AddrParseError) -> Self {
        TransportError::WriteError(e.to_string())
    }
}

/// Transform into a [`TransportError`]
impl From<mpsc::error::SendError<()>> for TransportError {
    fn from(e: SendError<()>) -> Self {
        TransportError::ShutdownError(e.to_string())
    }
}

/// Transform into a [`TransportError`]
impl From<JoinError> for TransportError {
    fn from(e: JoinError) -> Self {
        TransportError::ShutdownError(e.to_string())
    }
}
