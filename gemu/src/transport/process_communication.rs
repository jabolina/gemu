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
use crate::transport::{Listener, TransportError};
use std::borrow::BorrowMut;
use std::net::SocketAddr;

use mepa::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::task::{JoinError, JoinHandle};

/// The process structure responsible for holding all necessary information to send messages and to
/// correctly shutdown the underlying primitive.
struct ProcessCommunication {
    // The underlying TCP primitive implementation, we hold only the transmitter part, since we
    // only send messages using this structure.
    communication_tx: mepa::Sender,

    // The handle which is executing the polling method to receive new messages, we use this during
    // the shutdown in order to also close the spawned task.
    poll_join: JoinHandle<()>,

    // A channel used to communicate with the task which is polling messages. This is used to
    // notify about the shutdown process, so the task can stop.
    shutdown_tx: mpsc::Sender<()>,
}

/// Function responsible to receive new messages from the underlying receiver.
///
/// This method should execute during the whole application lifetime, receiving messages until the
/// signal is received through the channel notifying about the shutdown.
async fn poll<T: Listener>(listener: T, mut rx: mepa::Receiver, mut signal: mpsc::Receiver<()>) {
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

impl ProcessCommunication {
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
    pub(crate) async fn new<T>(port: usize, listener: T) -> transport::TransportResult<Self>
    where
        T: Listener + Send + Sync + 'static,
        <T as Listener>::Future: Send,
    {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (communication_rx, communication_tx) = mepa::create(port).await?;
        let poll_join = tokio::spawn(async move {
            poll(listener, communication_rx, shutdown_rx).await;
        });
        Ok(ProcessCommunication {
            communication_tx,
            poll_join,
            shutdown_tx,
        })
    }

    /// Send a the data to the destination.
    ///
    /// Will send the given data to the given destination using the underlying TCP connection. The
    /// data must be serializable in order to be written to the socket.
    ///
    /// # Errors
    ///
    /// This can fail for any error that may happen while handling the socket, for example, writing
    /// the data or establishing the connection.
    async fn send(
        &mut self,
        destination: SocketAddr,
        data: impl ToString,
    ) -> transport::TransportResult<()> {
        Ok(self.communication_tx.send(destination, data).await?)
    }

    /// Stops the current transport primitive.
    ///
    /// This will consume the structure, so after stopping is not possible to send message anymore.
    /// The polling task will also be stopped, so no new messages will be received.
    ///
    /// It would be nice to add this functionality while implementing the drop?
    ///
    /// # Errors
    ///
    /// This can fail if the task which is polling the message has panicked or is cancelled. Is
    /// also possible to fail while we send the shutdown signal through the channel.
    async fn stop(mut self) -> transport::TransportResult<()> {
        self.shutdown_tx.send(()).await?;
        self.poll_join.borrow_mut().await?;
        Ok(())
    }
}

/// Transform into a [`TransportError`]
impl From<mepa::Error> for TransportError {
    fn from(e: Error) -> Self {
        TransportError::WriteError(format!("{:?}", e))
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

#[cfg(test)]
mod tests {
    use crate::transport::process_communication::ProcessCommunication;
    use crate::transport::{Listener, TransportResult};
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::mpsc;

    struct DumbListener {
        tx: mpsc::Sender<String>,
    }
    impl Listener for DumbListener {
        type Future = Pin<Box<dyn Future<Output = TransportResult<()>> + Send>>;

        fn handle(&self, data: String) -> Self::Future {
            let sender = self.tx.clone();
            println!("Received: {}", data);
            Box::pin(async move {
                assert_eq!(data, String::from("hello"));
                let sent = sender.send(data).await;
                assert!(sent.is_ok());
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn create_send_receive_stop() {
        let (tx, mut rx) = mpsc::channel(1);
        let listener = DumbListener { tx: tx.clone() };
        let transport = ProcessCommunication::new(12345, listener).await;
        assert!(transport.is_ok());

        let mut transport = transport.unwrap();

        let response = transport
            .send("127.0.0.1:12345".parse().unwrap(), "hello")
            .await;
        assert!(response.is_ok());

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let data = rx.recv().await;
            assert!(data.is_some());
            assert_eq!(data.unwrap(), "hello");
        })
        .await;

        assert!(received.is_ok());

        let stopped = transport.stop().await;
        assert!(stopped.is_ok());
    }

    #[tokio::test]
    async fn should_return_error_binding() {
        let (tx, _) = mpsc::channel(1);
        let st_listener = DumbListener { tx: tx.clone() };
        let nd_listener = DumbListener { tx: tx.clone() };
        let st_transport = ProcessCommunication::new(12346, st_listener).await;
        let nd_transport = ProcessCommunication::new(12346, nd_listener).await;

        assert!(st_transport.is_ok());
        assert!(nd_transport.is_err());
        assert!(st_transport.unwrap().stop().await.is_ok());
    }
}
