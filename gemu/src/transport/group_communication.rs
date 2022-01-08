use crate::transport;
use crate::transport::{AsyncTrait, Listener, Sender, TransportError};
use abro::Error;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

pub(crate) struct GroupCommunication {
    communication_tx: abro::Sender,
}

async fn poll<'a, T: Listener<'a>>(
    listener: T,
    transport: abro::Receiver,
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

pub(crate) async fn new<'a, T>(
    configuration: abro::TransportConfiguration,
    listener: T,
) -> transport::TransportResult<transport::primitive::Transport<GroupCommunication>>
where
    T: Listener<'a> + 'static,
{
    let create = |shutdown_rx| async move {
        let (communication_tx, communication_rx) = abro::channel(configuration).await?;
        let poll_join = tokio::spawn(async move {
            poll(listener, communication_rx, shutdown_rx).await;
        });
        let primitive = GroupCommunication { communication_tx };
        Ok((primitive, poll_join))
    };
    Ok(transport::primitive::Transport::new(create).await?)
}

impl<'a> AsyncTrait<'a> for GroupCommunication {
    type Future = Pin<Box<dyn Future<Output = transport::TransportResult<()>> + Send + 'a>>;
}

impl<'a> Sender<'a> for GroupCommunication {
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

impl From<abro::Error> for TransportError {
    fn from(e: Error) -> Self {
        TransportError::GroupError(format!("{:?}", e))
    }
}
