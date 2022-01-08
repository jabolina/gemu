use mpsc::error::SendError;
use std::borrow::BorrowMut;
use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;
use tokio::task::{JoinError, JoinHandle};

use crate::transport;
use crate::transport::{AsyncTrait, Sender};

/// A generic structure to wrap any type of transport.
///
/// This structure will wrap a [`Sender`], so is possible to send messages and will spawn a thread
/// to poll for new messages. This structure is also responsible for a shutdown, stopping receiving
/// and sending any messages after stopping.
pub(crate) struct Transport<S> {
    // A structure that implements the [`Sender`] trait. This will be used to send new messages.
    sender: S,

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
async fn poll<F, T>(receiver: F, mut shutdown_rx: mpsc::Receiver<()>)
where
    F: FnOnce() -> T,
    T: Future<Output = ()>,
{
    let receiver = async move {
        receiver().await;
    };

    tokio::select! {
        // The polling method, in a happy way will execute forever without any errors.
        _ = receiver => {},

        // In the happy way, this is the future which will be completed first. When a signal is
        // received it means that we must stop polling for messages, so we just exit the select
        // automatically canceling the polling method.
        _ = shutdown_rx.recv() => {},
    }
}

impl<'a, S> Transport<S>
where
    S: Sender<'a>,
{
    /// Used to create a new transport primitive. This is used by the concrete transport
    /// implementation, so the transport wraps around the concrete implementation type `S`.
    ///
    /// This will receive as arguments the actual [`Sender`] and a future to poll for new messages.
    /// The [`Sender`] argument is the concrete transport implementation, when a message is sent it
    /// will be dispatched using the concrete sender.
    ///
    /// The second argument is a future which is responsible for receiving incoming messages.
    /// Usually this future will live the complete system life, always polling for new messages.
    pub(crate) fn new<F, T>(sender: S, listen: F) -> Transport<S>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Future<Output = ()> + Send,
    {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let poll_join = tokio::spawn(async move {
            poll(listen, shutdown_rx).await;
        });

        Transport {
            sender,
            poll_join,
            shutdown_tx,
        }
    }

    /// Stops the current transport primitive.
    ///
    /// This will consume the structure, so after stopping is not possible to send message anymore.
    /// The polling task will also be stopped, so no new messages will be received. Since the poll
    /// method must stop, it will stop only after receiving the signal, is possible that this takes
    /// some time to complete?
    ///
    /// It would be nice to add this functionality while implementing the drop trait?
    ///
    /// # Errors
    ///
    /// This can fail if the task which is polling the message has panicked or is cancelled. Is
    /// also possible to fail while we send the shutdown signal through the channel.
    pub(crate) async fn stop(mut self) -> transport::TransportResult<()> {
        self.shutdown_tx.send(()).await?;
        self.poll_join.borrow_mut().await?;
        Ok(())
    }
}

/// Implements the [`AsyncTrait`] for the current transport abstraction.
impl<'a, S> AsyncTrait<'a, transport::TransportResult<()>> for Transport<S>
where
    S: Sender<'a>,
{
    type Future = Pin<Box<dyn Future<Output = transport::TransportResult<()>> + Send + 'a>>;
}

impl<'a, S> Sender<'a> for Transport<S>
where
    S: Sender<'a>,
{
    /// Implements the [`Sender`] trait for the current transport abstraction.
    ///
    /// This will only pipe the method call to the concrete transport sender.
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future {
        Box::pin(async move { self.sender.send(destination, data).await })
    }
}

/// Transform into a [`TransportError`]
impl From<SendError<()>> for transport::TransportError {
    fn from(e: SendError<()>) -> Self {
        transport::TransportError::ShutdownError(e.to_string())
    }
}

/// Transform into a [`TransportError`]
impl From<JoinError> for transport::TransportError {
    fn from(e: JoinError) -> Self {
        transport::TransportError::ShutdownError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    /// Although is better to create the tests where the implementation is, all transports are
    /// tested here to avoid some code duplication. All tests follow the same structure of first
    /// creating the listener, creating the concrete implementation which is wrapped with the
    /// generic [`Transport`] structure, sending a messages, verifying it was received in the
    /// [`Listener`] implementation and then calling the stop method.
    ///
    /// To verify if the messages was actually closed, a 5 seconds timeout is defined.
    use crate::transport::{
        group_communication, process_communication, AsyncTrait, Listener, Sender,
    };
    use abro::TransportConfiguration;
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::mpsc;

    struct DumbListener {
        tx: mpsc::Sender<String>,
    }

    impl<'a> AsyncTrait<'a, ()> for DumbListener {
        type Future = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
    }

    /// Implements the [`Listener`] trait for the [`DumbListener`].
    ///
    /// This will only receive the message, assert that it is `"hello"` and publish to the channel.
    impl<'a> Listener<'a> for DumbListener {
        fn handle(&self, data: String) -> Self::Future {
            let sender = self.tx.clone();
            println!("Received: {}", data);
            Box::pin(async move {
                assert_eq!(data, String::from("hello"));
                let sent = sender.send(data).await;
                assert!(sent.is_ok());
            })
        }
    }

    /// Verify the simple process communication primitive.
    #[tokio::test]
    async fn process_create_send_receive_stop() {
        let (tx, rx) = mpsc::channel(1);
        let listener = DumbListener { tx: tx.clone() };
        let transport = process_communication::new(12345, listener).await;
        assert!(transport.is_ok());

        let mut transport = transport.unwrap();

        let response = transport.send("127.0.0.1:12345", "hello").await;
        assert!(response.is_ok());

        assert_message_was_received(rx).await;

        let stopped = transport.stop().await;
        assert!(stopped.is_ok());
    }

    /// Verified an error is returned from the create call.
    #[tokio::test]
    async fn process_should_return_error_binding() {
        let (tx, _) = mpsc::channel(1);
        let st_listener = DumbListener { tx: tx.clone() };
        let nd_listener = DumbListener { tx: tx.clone() };
        let st_transport = process_communication::new(12346, st_listener).await;
        let nd_transport = process_communication::new(12346, nd_listener).await;

        assert!(st_transport.is_ok());
        assert!(nd_transport.is_err());
        assert!(st_transport.unwrap().stop().await.is_ok());
    }

    /// Verify the group communication primitive.
    ///
    /// This test is ignored because it requires an etcd server running locally on port 2379.
    #[ignore]
    #[tokio::test]
    async fn group_create_send_receive_stop() {
        let (tx, rx) = mpsc::channel(1);
        let listener = DumbListener { tx: tx.clone() };
        let partition = format!("gemu-partition-{}", some_id());
        let configuration = TransportConfiguration::builder()
            .with_host("localhost")
            .with_port(2379)
            .with_partition(&partition)
            .build();
        assert!(configuration.is_ok());

        let transport = group_communication::new(configuration.unwrap(), listener).await;
        assert!(transport.is_ok());

        let mut transport = transport.unwrap();
        let response = transport.send(&partition, "hello").await;
        assert!(response.is_ok());

        assert_message_was_received(rx).await;

        let stopped = transport.stop().await;
        assert!(stopped.is_ok());
    }

    /// Returns an error if is not possible to establish a connection into etcd.
    #[tokio::test]
    async fn group_fail_etcd_not_available() {
        let (tx, _) = mpsc::channel(1);
        let listener = DumbListener { tx: tx.clone() };
        let configuration = TransportConfiguration::builder()
            .with_host("localhost")
            .with_port(1666)
            .with_partition("some-partition")
            .build();
        assert!(configuration.is_ok());

        let transport = group_communication::new(configuration.unwrap(), listener).await;
        assert!(transport.is_err());
    }

    fn some_id() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
    }

    async fn assert_message_was_received(mut rx: mpsc::Receiver<String>) {
        let received = tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let data = rx.recv().await;
            assert!(data.is_some());
            assert_eq!(data.unwrap(), "hello");
        })
        .await;

        assert!(received.is_ok());
    }
}
