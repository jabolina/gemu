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
        assert!(sender.send(destination, i).await.is_ok());
    }

    for _ in 0..15 {
        let data = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_some());
    }
}

#[tokio::test]
async fn send_receive_multiple_chunks() {
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

    // The underlying buffer have 8Kb size, so we will send 20 chunks with 512b size each, which
    // sums up to 10240 bytes.
    for _ in 0..20 {
        let data_chunk = String::from_utf8([97; 512].to_vec()).expect("Data array");
        assert!(sender.send(destination, data_chunk).await.is_ok());
    }

    for _ in 0..20 {
        let data = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_some());
        let data = data.unwrap();
        assert_eq!(data.len(), 512);
    }
}

#[tokio::test]
async fn send_receive_single_large_chunk() {
    const CHUNK_SIZE: usize = 2 * 8 * 1024;
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

    // The underlying buffer have 8Kb size, so we will send a single chunk with double the buffer
    // size.
    let data_chunk = String::from_utf8([97; CHUNK_SIZE].to_vec()).expect("Data array");
    assert!(sender.send(destination, data_chunk).await.is_ok());

    let data = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
    assert!(data.is_ok());
    let data = data.unwrap();
    assert!(data.is_some());
    let data = data.unwrap();
    assert_eq!(data.len(), CHUNK_SIZE);
}

/// Binding can return an error.
#[tokio::test]
async fn duplicate_bind_returns_error() {
    let st_rx = mepa::create_rx(12355).await;
    let nd_rx = mepa::create_rx(12355).await;

    assert!(st_rx.is_ok());
    assert!(nd_rx.is_err());
}
