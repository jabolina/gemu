use crate::server::TcpServer;
use std::future::Future;
use tokio::net::ToSocketAddrs;
use tokio::signal;
use tokio::sync::mpsc;

/// The unique piece that the user interacts with, used to listen for incoming messages and to
/// send messages to other sockets. The underlying implementation is using TCP sockets.
pub struct Transport {
    // A multiple producer and single consumer channel from tokio. The single consumer is the
    // transport structure itself, and for each incoming connection a new producer is created.
    // This is used gather messages sent by other peers using a single channel, avoiding the need
    // for callbacks passed back and forth.
    data_rx: mpsc::Receiver<String>,

    // The underlying server, the server holds the stream and accepts connections.
    server: TcpServer,
}

/// Entrypoint for creating a new [`Transport`] structure.
///
/// The argument is the port in which the server will bind. No validation is executed here to
/// verify if is an accepted door. The response from this method is a [`Transport`] structure which
/// the user can use to listen messages and to send messages.
///
/// Note that a socket will be created and bind itself just after this method is called, the will
/// be in use, but in the current implementation the socket will only start to accept connections
/// _after_ the [`Transport.poll`] method is called, so a better approach is to create the structure
/// and start polling right away.
///
/// # Examples
///
/// ```
/// use mepa::create_server;
///
/// #[tokio::main]
/// async fn main() {
///     let transport = create_server(13666).await;
/// #    assert!(transport.is_ok());
///     // transport.poll...
/// }
/// ```
///
/// # Errors
///
/// This method will fail if is not possible to bind to the port given as argument.
pub async fn create_server(port: usize) -> crate::Result<Transport> {
    // The channel will hold 1024 messages before starting blocking producers.
    let (data_tx, data_rx) = mpsc::channel(1024);
    let server = crate::server::bind(port, data_tx).await?;
    Ok(Transport { data_rx, server })
}

impl Transport {
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
                if let Some(data) = rx.recv().await {
                    cb(data).await;
                }
            }
        }

        // In theory this should run forever, in a perfect scenario, and this only should stop after
        // receiving the shutdown signal, then both the poll and listen_messages will be canceled.
        tokio::select! {
            res = self.server.poll() => {
                if let Err(e) = res {
                    eprintln!("Failed: {:?}", e);
                }
            }
            _ = signal::ctrl_c() => {}
            _ = listen_messages(&mut self.data_rx, f) => {}
        }

        self.server.shutdown();
        Ok(())
    }

    /// Used to send a message to another address.
    ///
    /// The data must be serializable in order to be written to the socket as bytes.
    pub async fn send<T: ToSocketAddrs>(
        &self,
        destination: T,
        data: impl ToString,
    ) -> crate::Result<()> {
        Ok(())
    }
}
