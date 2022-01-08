/// All tests written here are following the same methodology. We are creating a sender and
/// receiver, then we proceed to spawn the transmitter in another thread to transmit the incoming
/// data and publish it back through a mpsc channel.
///
/// Meanwhile the sender will proceed to send the data to the receiver, we have different tests for
/// different data types:
///
/// 1. Sending thousands of small data
/// 2. Sending few chunks of 512 bytes
/// 3. Sending a single large chunk
///
/// Then we block and listen the mpsc listener, where we must receive the complete data that was
/// published previously. A 5 second timeout is used so we do not hang forever in these tests.

#[tokio::test]
async fn send_and_receive_messages() {
    let transport = mepa::channel(0).await;
    assert!(transport.is_ok());

    let (mut sender, mut receiver) = transport.unwrap();
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

    for i in 0..1500 {
        assert!(sender.send(destination, i.to_string()).await.is_ok());
    }

    for _ in 0..1500 {
        let data = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_some());
    }
}

#[tokio::test]
async fn send_receive_multiple_chunks() {
    let transport = mepa::channel(0).await;
    assert!(transport.is_ok());

    let (mut sender, mut receiver) = transport.unwrap();
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
    let transport = mepa::channel(0).await;
    assert!(transport.is_ok());

    let (mut sender, mut receiver) = transport.unwrap();
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

/// Binding will return an error.
#[tokio::test]
async fn duplicate_bind_returns_error() {
    let st_rx = mepa::create_rx(12355).await;
    let nd_rx = mepa::create_rx(12355).await;

    assert!(st_rx.is_ok());
    assert!(nd_rx.is_err());
}
