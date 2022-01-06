use crate::transport;
use crate::transport::{Listener, TransportError};
use abro::Error;
use std::fmt::format;
use std::future::Future;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

struct GroupCommunication<T: Listener> {
    listener: T,
    transport: abro::Transport,
    //poll_join: JoinHandle<()>,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: mpsc::Receiver<()>,
}

async fn poll<T: Listener>(
    listener: &T,
    transport: &abro::Transport,
    mut signal: mpsc::Receiver<()>,
) {
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

impl<T> GroupCommunication<T>
where
    T: Listener + Send + Sync + 'static,
    <T as Listener>::Future: Send,
{
    pub(crate) async fn new(
        configuration: abro::TransportConfiguration,
        listener: T,
    ) -> transport::TransportResult<GroupCommunication<T>> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let transport = abro::Transport::new(configuration).await?;
        let group = GroupCommunication {
            listener,
            transport,
            //poll_join,
            shutdown_tx,
            shutdown_rx,
        };

        let poll_join = tokio::spawn(GroupCommunication::poll(&group));
        Ok(group)
    }

    async fn poll(&self) {
        let _ = self
            .transport
            .listen(|data| async {
                if let Ok(data) = data {
                    let _ = self.listener.handle(data).await;
                }
                Ok(true)
            })
            .await;
    }
}

impl From<abro::Error> for TransportError {
    fn from(e: Error) -> Self {
        TransportError::GroupError(format!("{:?}", e))
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn create_send_receive_stop() {}
}
