use std::borrow::BorrowMut;
use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::transport;
use crate::transport::{AsyncTrait, Sender};

pub(crate) struct Transport<S> {
    sender: S,
    // The handle which is executing the polling method to receive new messages, we use this during
    // the shutdown in order to also close the spawned task.
    poll_join: JoinHandle<()>,

    // A channel used to communicate with the task which is polling messages. This is used to
    // notify about the shutdown process, so the task can stop.
    shutdown_tx: mpsc::Sender<()>,
}

impl<'a, S> Transport<S>
where
    S: Sender<'a>,
{
    pub(crate) async fn new<F, T>(create: F) -> transport::TransportResult<Transport<S>>
    where
        F: FnOnce(mpsc::Receiver<()>) -> T,
        T: Future<Output = transport::TransportResult<(S, JoinHandle<()>)>>,
    {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (sender, poll_join) = create(shutdown_rx).await?;
        Ok(Transport {
            sender,
            poll_join,
            shutdown_tx,
        })
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
    pub(crate) async fn stop(mut self) -> transport::TransportResult<()> {
        self.shutdown_tx.send(()).await?;
        self.poll_join.borrow_mut().await?;
        Ok(())
    }
}

impl<'a, S> AsyncTrait<'a> for Transport<S>
where
    S: Sender<'a> + 'a,
{
    type Future = Pin<Box<dyn Future<Output = transport::TransportResult<()>> + Send + 'a>>;
}

impl<'a, S> Sender<'a> for Transport<S>
where
    S: Sender<'a> + 'a,
{
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future {
        Box::pin(async move { self.sender.send(destination, data).await })
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::{
        group_communication, process_communication, AsyncTrait, Listener, Sender, TransportResult,
    };
    use abro::TransportConfiguration;
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::mpsc;

    struct DumbListener {
        tx: mpsc::Sender<String>,
    }

    impl<'a> AsyncTrait<'a> for DumbListener {
        type Future = Pin<Box<dyn Future<Output = TransportResult<()>> + Send + 'a>>;
    }

    impl<'a> Listener<'a> for DumbListener {
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
    async fn process_create_send_receive_stop() {
        let (tx, mut rx) = mpsc::channel(1);
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

    #[ignore]
    #[tokio::test]
    async fn group_create_send_receive_stop() {
        let (tx, mut rx) = mpsc::channel(1);
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
