//! Some integration tests using the [`abro::Transport`] primitive.
//!
//! Note that the tests are ignored since is required an etcd server running to properly work.
use abro::{Transport, TransportConfiguration};

/// This test is creating two peers that are bound to different partitions. The second peer created
/// broadcast the message to the partition of the first peer, then we set the first peer to start
/// listening to received messages, and the message sent previously by the second peer is received
/// by the first peer.
///
/// We are testing that we in fact do receive messages that were broadcast and that we receive
/// messages that were broadcast _before_ we started listening.
#[ignore]
#[tokio::test]
async fn send_and_receive_message() {
    let content = "HELLO";
    let mut st_transport = create_transport("send-and-receive-1").await;
    let mut nd_transport = create_transport("send-and-receive-2").await;

    let message = abro::Message::new("send-and-receive-1", content);
    let sent = nd_transport.send(message).await;

    assert!(sent.is_ok());

    let spawned = tokio::spawn(async move {
        tokio::time::timeout(std::time::Duration::from_secs(10), async move {
            st_transport
                .listen(|data| async {
                    assert!(data.is_ok());

                    let data = data.unwrap();
                    assert_eq!(data, "HELLO");
                    Ok(false)
                })
                .await
        })
        .await
    })
    .await
    .unwrap();
    assert!(spawned.is_ok());
}

/// This test is a simples validation that we return an [`abro::Error`] when we are not able to
/// connect to the etcd server when creating a new [`abro::Transport`] primitive.
#[ignore]
#[tokio::test]
async fn fail_when_etcd_unreachable() {
    let configuration = TransportConfiguration::builder()
        .with_host("localhost")
        .with_port(1111)
        .with_partition("failed-connection")
        .build();

    assert!(configuration.is_ok());
    let configuration = configuration.unwrap();

    let transport = Transport::new(configuration).await;

    assert!(transport.is_err());
}

async fn create_transport(partition: &str) -> Transport {
    let configuration = TransportConfiguration::builder()
        .with_host("localhost")
        .with_port(2379)
        .with_partition(partition)
        .build();

    assert!(configuration.is_ok());
    let configuration = configuration.unwrap();

    let transport = Transport::new(configuration).await;

    assert!(transport.is_ok());
    transport.unwrap()
}
