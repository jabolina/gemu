/// Here we are creating both the receiver and sender. The receiver is spawned in another thread
/// along with a mpsc sender channel. The sender will publish 15 messages and then use the mpsc
/// receiver to wait for the messages sent previously.
/// There is a lazy person workaround here, if we send all messages at once, the reader can read
/// everything in a single chunk, so this test could never finish, so we added a 5s timeout.
#[tokio::test]
async fn send_and_receive_messages() {
    let transport = mepa::create(0).await;
    assert!(transport.is_ok());

    let (mut receiver, mut sender) = transport.unwrap();
    let destination = receiver.local_address();
    let (tx, mut rx) = tokio::sync::mpsc::channel(15);

    assert!(destination.is_ok());
    let destination = destination.unwrap();

    tokio::spawn(async move {
        async fn publish(s: String, tx: tokio::sync::mpsc::Sender<String>) {
            assert!(tx.send(s).await.is_ok());
        }
        assert!(receiver.poll(|s| publish(s, tx.clone())).await.is_ok());
    });

    for i in 0..15 {
        // If we directly send all of our messages at once, all data will be buffered into a single
        // chunk on the receiver side. This is sleep is the super lazy fix.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(sender.send(destination, i).await.is_ok());
    }

    for _ in 0..15 {
        let data = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_some());
    }
}

/// Binding can return an error.
#[tokio::test]
async fn duplicate_bind_returns_error() {
    let st_rx = mepa::create_rx(12355).await;
    let nd_rx = mepa::create_rx(12355).await;

    assert!(st_rx.is_ok());
    assert!(nd_rx.is_err());
}
