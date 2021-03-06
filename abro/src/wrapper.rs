//! Write message to the etcd server and listen for changes.
//!
//! Here we will use the `etcd` client itself in order to write values to the KV store and
//! to listen for changes that are applied to a specific key. Using this wrapper the transport
//! will be able to broadcast and receive messages, so its the wrapper responsibility to notify
//! the transport when changes are applied.
//!
//! In this context, `peer` means a node connected to the etcd server. Multiple `peers` means that
//! we have a collection of `peer` connected to the etcd server. A `partition` means that we have
//! one or more `peer` connected to the etcd server using the same [`EtcdWrapper::partition`] key.

use async_stream::try_stream;
use etcd_client as etcd;
use etcd_client::Error;
use std::str;
use tokio_stream::Stream;

/// This is the wrapper around the `etcd` KV client.
///
/// Using this structure we can write to the underlying KV store, in order to simulate the atomic
/// broadcast primitive. Note that this client is used for the only purpose of sending messages,
/// is not possible to receive messages using the [`EtcdWriter`].
pub(crate) struct EtcdWriter {
    // The client for the underlying etcd KV.
    client: etcd::KvClient,
}

/// This is a wrapper around the actual `etcd` client.
///
/// Although this structure holds the client itself, it will be used only for watching for new
/// messages. Wrapping around the client itself, we can create a [`WatchClient`] and listen for
/// changes without the need for a mutable reference for self.
///
/// [`WatchClient`]: etcd::WatchClient,
pub(crate) struct EtcdReceiver {
    // The etcd connection, which will be used to watch for changes that are applied in the current
    // peer partition.
    client: etcd::Client,

    // The partition in which the peer in bound. This is used to watch for changes that happen
    // in the current partition only. This partition must be a complete match over the key in
    // the etcd KV store, meaning that we are not watching for prefixes, we are watching the
    // complete key.
    partition: String,
}

impl EtcdWriter {
    /// Write the value to a given partition. The value *must* must implement the [`Into<String>`]
    /// trait so it can be serialized before sending using the [`EtcdWrapper::etcd`] client.
    ///
    /// This is used only internally, where the [`abro::transport::Transport`] struct abstract
    /// everything for the user, and let available only a send/receive primitive.
    pub(crate) async fn write(
        &mut self,
        destination: &str,
        value: impl Into<String>,
    ) -> crate::Result<()> {
        match self.client.put(destination, value.into(), None).await {
            Err(e) => Err(crate::Error::SendError(e.to_string())),
            Ok(_) => Ok(()),
        }
    }
}

impl EtcdReceiver {
    /// Create a stream of changes applied to the peer current partition.
    ///
    /// A peer will only receive events of changes that occur since the last compaction, this is
    /// the behavior from the etcd, I suppose is just not possible to watch for events that happened
    /// before the compaction. The peer will start watching for every change applied since the last
    /// compaction. The watch API makes three guarantees, that can be found in the documentation:
    ///
    /// 1. Ordered - events are ordered by revision;
    /// 2. Reliable - no subsequence of events will be dropped;
    /// 3. Atomic - a list of events contains a complete revision.
    ///
    /// Using this API will simulate that all peers are receiving a message that was atomically
    /// broadcast to a specific partition. The transport will start watching for changes at the
    /// beginning and will publish any changes that are applied. All peers will receive the same
    /// sequence of events in the same order.
    ///
    /// # Errors
    ///
    /// We are not dealing with any reconnection at this point, if our client disconnects from
    /// the etcd server we do not know what will happen.
    ///
    /// # Disclaimer
    ///
    /// In this approach using the last revision since the last compaction can lead to receiving
    /// duplicated message after restarts. If a peer start listening and processes message `a`, `b`,
    /// for some reason it crashes and restarts, when it starts to listen again it no compaction
    /// occurred in the etcd server, the peer will receive messages `a` and `b` again. This is a
    /// complex exactly-once deliver problem.
    ///
    /// We can change this behavior by not using the `start_revision` argument and start to only
    /// listen to changes when the peer just connects, but then again, this can lead to not
    /// receiving some of the events that happened while the peer was restarting.
    ///
    /// Probably the best approach is to persist the revision value in a persistent storage and
    /// retrieve the value when the peer start again. This is fun because we can use etcd for this,
    /// after receiving the event we start a transaction, we process the message and persist the
    /// revision value into etcd itself using an unique identifier for the current peer and then
    /// commit the transaction. Although I feel that this could not be enough, for example, the
    /// Kafka team solution for exactly-once semantics is somewhat complex.
    #[allow(unused_must_use)]
    pub(crate) async fn watch<'a>(&'a self) -> impl Stream<Item = crate::Result<String>> + 'a {
        let mut watch_client = self.client.watch_client();
        try_stream! {
            // We will start watching from the very first revision since the last compaction,
            // so we define the starting revision to `1`. The value `0` indicates the absence
            // of any value, which is the default value for the proto3.
            let options = etcd::WatchOptions::new()
                .with_start_revision(1);

            // Here we start watching the specified partition key. Should we also offer the possibility
            // to watch a prefix of a key? We could create something similar to an exchange here.
            let (mut watcher, mut stream) = watch_client.watch(self.partition.as_str(), Some(options)).await?;
            while let Some(events) = stream.message().await? {
                // If the events are cancelled somehow, we will also cancel our watcher. I suppose
                // this means that the stream will end?
                if events.canceled() {
                    watcher.cancel();
                }

                for event in events.events() {
                    // We are interested only in changes applied to the KV. The value applied is
                    // turned into String and will be propagated back to the transport layer.
                    // Is the user responsibility to serialize the content back to a structure.
                    if let Some(change) = event.kv() {
                        let value = change.value_str()?;
                        yield String::from(value);
                    }
                }
            }
        }
    }
}

/// Transform the [`etcd::Error`] into our [`crate::Error`] object. This is just a simple
/// transformation where the original error is transformed into a string so our custom
/// error type can cary a context.
impl From<etcd::Error> for crate::Error {
    fn from(e: Error) -> Self {
        crate::Error::InternalError(e.to_string())
    }
}

/// Connect to the given etcd servers and create a new [`EtcdWrapper`].
///
/// The address will be passed directly to the client connection, so the etcd library will
/// handle all values directly.
///
/// # Errors
///
/// This method can fail if we are unable to connect to the etcd server using the given address.
///
pub(crate) async fn connect(
    address: &str,
    partition: &str,
) -> crate::Result<(EtcdWriter, EtcdReceiver)> {
    let client = etcd::Client::connect([address], None).await?;
    let writer = EtcdWriter {
        client: client.kv_client(),
    };
    let receiver = EtcdReceiver {
        client,
        partition: String::from(partition),
    };
    Ok((writer, receiver))
}

#[cfg(test)]
/// All tests here are ignored because they require a running etcd server to be verified.
/// If you have a local etcd server running is possible to run each test specifically.
mod tests {
    use crate::wrapper::connect;

    /// This is a super simple test, here we are create a peer that does a single write.
    /// Every step is verified that is a success.
    #[ignore]
    #[tokio::test]
    async fn should_connect_and_write() {
        let partition = "connect-write";
        let wrapper = connect("localhost:2379", partition).await;
        assert!(wrapper.is_ok());

        let (mut tx, _) = wrapper.unwrap();
        let request = tx.write(partition, "hello").await;

        assert!(request.is_ok());
    }

    /// This is another simple test that we are verifying that if the etcd server is unreachable
    /// we do in fact return an error.
    #[ignore]
    #[tokio::test]
    async fn should_return_err_if_not_connected() {
        let unconnected = connect("localhost:1111", "not").await;

        assert!(unconnected.is_err());
    }

    /// This is a bit more complex test. Here we are creating two peers for the same partition,
    /// peers `st_peer` and `nd_peer`. Then the peer `nd_peer` write the value `"horray!"` to
    /// the partition, next we use the `st_peer` to start listening for changes _after_ the write
    /// and we expect to receive an event notifying that a change was applied to the current
    /// partition and it contains the value `"horray!"`.
    ///
    /// This listen have a timeout of 10 seconds, if no event is received by then we define that
    /// the test has failed.
    #[ignore]
    #[tokio::test]
    async fn should_connect_listen_and_write() {
        use futures_util::pin_mut;
        use futures_util::stream::StreamExt;
        let partition = "read-write";
        let published_data = "horray!";
        let address = "localhost:2379";

        let wrapper = connect(address, partition).await;

        assert!(wrapper.is_ok());

        let (mut tx, rx) = wrapper.unwrap();

        let write = tx.write(partition, published_data).await;
        assert!(write.is_ok());

        let response = tokio::spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async move {
                let stream = rx.watch().await;
                pin_mut!(stream);
                while let Some(v) = stream.next().await {
                    assert!(v.is_ok());
                    let content = v.unwrap();
                    assert_eq!(content, published_data);
                    break;
                }
            })
            .await
        })
        .await
        .unwrap();

        assert!(response.is_ok());
    }
}
