use crate::client::ClientManager;
use crate::server::TcpServer;
use std::future::Future;
use std::net::SocketAddr;
use tokio::signal;
use tokio::sync::mpsc;

/// This is the structure used ot receive message from other peers. This structure is used basically
/// to bind the TCP socket and start receiving messages. Since a socket is used only a single
/// instance will exists.
pub struct Receiver {
    // A multiple producer and single consumer channel from tokio. The single consumer is the
    // transport structure itself, and for each incoming connection a new producer is created.
    // This is used gather messages sent by other peers using a single channel, avoiding the need
    // for callbacks passed back and forth.
    data_rx: mpsc::Receiver<String>,

    // The underlying server, the server holds the stream and accepts connections.
    server: TcpServer,
}

/// This is the structure used to send messages to other peers. The underlying implementation will
/// create a connection pool to avoid establishing a connection any time a new message is sent, so
/// the best approach is to use only a single instance of the [`Sender`] during all times.
pub struct Sender {
    // The client is used to effectively send a message. This client manages all client connections.
    client: ClientManager,
}

/// This is used to create the tuple of a [`Receiver`] and a [`Sender`].
///
/// The argument is the port in which the server will bind. No validation is executed here to
/// verify if is an accepted door. The response from this method is a tuple which the user can use
/// to listen and send messages.
///
/// Note that a socket will be created and bind itself just after this method is called, the socket
/// will be in use, but in the current implementation the socket will only start to accept
/// connections _after_ the [`Receiver.poll`] method is called, so a better approach is to create
/// the structure and start polling right away.
///
/// # Examples
///
/// ```
/// use mepa::channel;
///
/// #[tokio::main]
/// async fn main() {
///     let transport = channel(13666).await;
/// #    assert!(transport.is_ok());
///     let (rx, tx) = transport.unwrap();
///     // rx.poll...
///     // tx.send...
/// }
/// ```
///
/// # Errors
///
/// This method will fail if is not possible to bind to the port given as argument.
pub async fn channel(port: usize) -> crate::Result<(Sender, Receiver)> {
    let receiver = create_rx(port).await?;
    let sender = create_tx();
    Ok((sender, receiver))
}

/// Create a [`Receiver`] that bind to the given port.
///
/// This method creates only the receiver part, this could be used when sending messages is not
/// necessary.
///
/// # Examples
///
/// ```
/// use mepa::create_rx;
///
/// #[tokio::main]
/// async fn main() {
///     let rx = create_rx(13666).await;
/// #    assert!(rx.is_ok());
///     // rx..unwrap().poll...
/// }
/// ```
///
/// # Errors
///
/// This method will fail if is not possible to bind to the port given as argument.
pub async fn create_rx(port: usize) -> crate::Result<Receiver> {
    // The channel will hold 1024 messages before starting blocking producers.
    let (data_tx, data_rx) = mpsc::channel(1024);
    let server = crate::server::bind(port, data_tx).await?;
    Ok(Receiver { data_rx, server })
}

/// Creates a [`Sender`] to only send messages.
///
/// This method creates only the receiver part, this could be used when receiving messages is not
/// required.
///
/// # Examples
///
/// ```
/// use mepa::create_tx;
///
/// fn main() {
///     let tx = create_tx();
///     // tx.send...
/// }
/// ```
pub fn create_tx() -> Sender {
    Sender {
        client: ClientManager::new(),
    }
}

impl Receiver {
    /// Start polling, accepting new socket connections and publishing the received messages.
    ///
    /// This method receive as argument a function that will be called every time a new message is
    /// received. Note that this method will block after is called and will only return when either
    /// the underlying server stops polling or if a signal is received to stop.
    pub async fn poll<F, T>(&mut self, f: F) -> crate::Result<()>
    where
        F: Fn(String) -> T,
        T: Future<Output = ()>,
    {
        // We create an internal function to receive message so we can pass the rx as a reference
        // to the current function, if a dedicated method exists is not possible to execute since
        // self would need to be mutable in more than one place.
        // This method will run "forever" until tokio cancels it from within the select! macro.
        async fn listen_messages<F, T>(rx: &mut mpsc::Receiver<String>, cb: F)
        where
            F: Fn(String) -> T,
            T: Future<Output = ()>,
        {
            loop {
                // The recv method is cancel safe, so there is no problem using it with a select!.
                if let Some(data) = rx.recv().await {
                    cb(data).await;
                }
            }
        }

        // In theory this should run forever, in a perfect scenario, and this only should stop after
        // receiving the shutdown signal, then both the poll and listen_messages will be canceled.
        tokio::select! {
            // This should only happen if is not possible to accept more connections.
            res = self.server.poll() => {
                if let Err(e) = res {
                    eprintln!("Failed: {:?}", e);
                }
            }
            _ = signal::ctrl_c() => {}
            // This should not happen, since the method runs forever.
            _ = listen_messages(&mut self.data_rx, f) => {}
        }

        self.server.shutdown().await;
        Ok(())
    }

    pub fn local_address(&self) -> crate::Result<SocketAddr> {
        self.server.local_address()
    }
}

impl Sender {
    /// Used to send a message to another address.
    ///
    /// The data must be serializable in order to be written to the socket as bytes.
    ///
    /// # Errors
    ///
    /// The underlying implementation is using TCP sockets for communication, so a connection must
    /// be established before sending the message, is possible that an error can occur while
    /// establishing the connection with the destination socket.
    ///
    /// If the connection is established successfully, is possible that a the system can fail while
    /// writing the data to the socket.
    pub async fn send(
        &mut self,
        destination: SocketAddr,
        data: impl Into<String>,
    ) -> crate::Result<()> {
        self.client.write_to(&destination.to_string(), data).await
    }
}
