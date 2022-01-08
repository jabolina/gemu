use crate::transport;
use crate::transport::{Listener, TransportError};
use abro::Error;
use std::borrow::BorrowMut;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

struct GroupCommunication {
    communication_tx: abro::Sender,
    poll_join: JoinHandle<()>,
    shutdown_tx: mpsc::Sender<()>,
}

async fn poll<T: Listener>(listener: T, transport: abro::Receiver, mut signal: mpsc::Receiver<()>) {
    let polling = async move {
        let _ = transport
            .listen(|data| async {
                if let Ok(data) = data {
                    let _ = listener.handle(data).await;
                }
                Ok(true)
            })
            .await;
    };

    tokio::select! {
        _ = polling => {}
        _ = signal.recv() => {}
    }
}

impl GroupCommunication {
    pub(crate) async fn new<T>(
        configuration: abro::TransportConfiguration,
        listener: T,
    ) -> transport::TransportResult<GroupCommunication>
    where
        T: Listener + Send + Sync + 'static,
        <T as Listener>::Future: Send,
    {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (communication_tx, communication_rx) = abro::channel(configuration).await?;
        let poll_join = tokio::spawn(async move {
            poll(listener, communication_rx, shutdown_rx).await;
        });
        let group = GroupCommunication {
            communication_tx,
            poll_join,
            shutdown_tx,
        };
        Ok(group)
    }

    async fn send(
        &mut self,
        destination: &str,
        data: impl Into<String>,
    ) -> transport::TransportResult<()> {
        let message = abro::Message::new(destination, data);
        Ok(self.communication_tx.send(message).await?)
    }

    async fn stop(mut self) -> transport::TransportResult<()> {
        self.shutdown_tx.send(()).await?;
        self.poll_join.borrow_mut().await?;
        Ok(())
    }
}

impl From<abro::Error> for TransportError {
    fn from(e: Error) -> Self {
        TransportError::GroupError(format!("{:?}", e))
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::group_communication::GroupCommunication;
    use crate::transport::{Listener, TransportResult};
    use abro::TransportConfiguration;
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

    #[ignore]
    #[tokio::test]
    async fn create_send_receive_stop() {
        let (tx, mut rx) = mpsc::channel(1);
        let listener = DumbListener { tx: tx.clone() };
        let partition = format!("gemu-partition-{}", some_id());
        let configuration = TransportConfiguration::builder()
            .with_host("localhost")
            .with_port(2379)
            .with_partition(&partition)
            .build();
        assert!(configuration.is_ok());

        let transport = GroupCommunication::new(configuration.unwrap(), listener).await;
        assert!(transport.is_ok());

        let mut transport = transport.unwrap();
        let response = transport.send(&partition, "hello").await;
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

    fn some_id() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
    }
}
