use crate::shutdown::Shutdown;
use crate::tcp_connection::TcpConnection;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{broadcast, mpsc};

/// Responsible for holding the TCP stream itself, this is the underlying TCP server in which the
/// current peer is bound. This structure is responsible to accept connection from other peers and
/// passing the received messages to the channel.
///
/// This structure is also responsible to execute a graceful shutdown, meaning that after the
/// shutdown method is called, this structure must wait for any work and close all accepted
/// connections. During this time of shutdown, no new connection should be accepted.
pub struct TcpServer {
    // The TCP stream itself, used to verify for incoming connection requests.
    listener: tokio::net::TcpListener,

    // A channel to write the data received from the accepted connections. In the current project
    // there is no ordering guarantee, so there is no problem if multiple connections are writing
    // at the same time.
    data_tx: mpsc::Sender<String>,

    // Used to manage the shutdown process.
    shutdown_tx: broadcast::Sender<()>,

    // Both channel are used to execute a graceful server shutdown, were we will wait for all
    // handlers to be dropped so only then we will finish the server itself. We clone the sender
    // part to all spawned handler, at shutdown we use the receiver to know that everything was
    // dropped.
    graceful_rx: mpsc::Receiver<()>,
    graceful_tx: mpsc::Sender<()>,
}

/// A handler is a handler for a specific incoming connection that is established successfully.
/// This handler will manage only a single connection, multiple handlers can exist during the
/// socket lifetime.
struct Handler {
    // The accepted TCP connection, through this connection will keep reading for any incoming data.
    connection: TcpConnection,

    // Used to handle the shutdown process.
    shutdown: Shutdown,

    // Used to write any received data.
    data_tx: mpsc::Sender<String>,

    // Used to know that the handler was dropped.
    _scope: mpsc::Sender<()>,
}

/// Through this step we are able to create a new [`tokio::net::TcpListener`] that will bind to the
/// port given as parameter. The second argument is a channel, for every new accepted connection a
/// new producer is created in order to publish the received data back to the single consumer.
///
/// # Errors
///
/// This method will fail only if is not possible to bind to the door passed as argument.
pub(crate) async fn bind(port: usize, data_tx: mpsc::Sender<String>) -> crate::Result<TcpServer> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let (graceful_tx, graceful_rx) = mpsc::channel(1);
    let listener = tokio::net::TcpListener::bind(&format!("127.0.0.1:{}", port)).await?;

    Ok(TcpServer {
        listener,
        data_tx,
        shutdown_tx,
        graceful_rx,
        graceful_tx,
    })
}

impl TcpServer {
    /// Start running the server.
    ///
    /// This will run forever receiving incoming connections and starting handlers to deal with
    /// each new connection.
    ///
    /// # Errors
    ///
    /// Because of the error handling adopted while accepting incoming connections is possible
    /// that we can stop if too many errors happen when accepting new connections. An exponential
    /// backoff is implemented in order to avoid retries too soon.
    pub(crate) async fn poll(&mut self) -> crate::Result<()> {
        loop {
            // Tries to accept new connections. Because of the error handling that happens inside
            // the method is possible that we fail and stop up to the point of closing the server.
            let socket = self.accept().await?;
            let mut handler = Handler {
                connection: TcpConnection::new_reader(socket),
                shutdown: Shutdown::new(self.shutdown_tx.subscribe()),
                data_tx: self.data_tx.clone(),
                _scope: self.graceful_tx.clone(),
            };
            tokio::spawn(async move {
                if let Err(e) = handler.process().await {
                    eprintln!("Connection error: {:?}", e);
                }
            });
        }
    }

    pub(crate) fn local_address(&self) -> crate::Result<SocketAddr> {
        let address = self.listener.local_addr()?;
        Ok(address)
    }

    /// Starts the shutdown process, this method may not return right away, since it can depend
    /// on multiple accepted connections to be finished.
    pub(crate) async fn shutdown(&mut self) {
        drop(self.shutdown_tx.to_owned());
        drop(self.shutdown_tx.to_owned());

        // Here we wait for all Handlers to go out of scope before returning.
        let _ = self.graceful_rx.recv().await;
    }

    /// Accept inbound connections.
    ///
    /// Using an exponential backoff to handle errors while accepting connections. After 6 tries
    /// we give up and return a [`crate::Error`].
    async fn accept(&mut self) -> crate::Result<TcpStream> {
        let mut backoff = 1;

        loop {
            match self.listener.accept().await {
                Ok((socket, _)) => return Ok(socket),
                Err(e) => {
                    if backoff > 64 {
                        return Err(crate::Error::SocketError(e.to_string()));
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }
}

impl Handler {
    /// Process the incoming connection.
    ///
    /// This method will return only if the connection is closed by the other half or if the server
    /// started the shutdown process.
    ///
    /// While the connection is established, we keep reading the connection to verify if there is
    /// any data available. The data will then be written to the [`tokio::sync::mpsc::Sender`] so
    /// it can be consumed by the user.
    async fn process(&mut self) -> crate::Result<()> {
        while !self.shutdown.is_shut() {
            let data = tokio::select! {
                res = self.connection.read() => res?,
                _ = self.shutdown.wait_shutdown() => {
                    return Ok(())
                }
            };

            let data = match data {
                Some(v) => v,
                None => return Ok(()),
            };

            // If we failed here, we can safely return, because it means that the receiver is closed
            // meaning that the server will shutdown at any time.
            self.data_tx.send(data).await?;
        }

        Ok(())
    }
}

/// Transforms the error that can occur while sending a message through the
/// [`tokio::sync::mpsc::Sender`] to the expected [`crate::Error`] format.
impl From<mpsc::error::SendError<String>> for crate::Error {
    fn from(e: SendError<String>) -> Self {
        crate::Error::SocketError(e.to_string())
    }
}
