use crate::tcp_connection::TcpConnection;
use std::collections::{HashMap, VecDeque};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::RwLock;

/// The [`ClientManager`] structure is responsible for managing all TCP client connections that are
/// established during the application lifetime. A connection pool will be created and connections
/// can be reused when multiple messages are sent to the same destination.
///
/// For each destination, a maximum of 10 connections can be kept in the pool, so is possible to
/// write multiple messages directly, and the manager will handle the connections. If more messages
/// are sent and there is no [`TcpClient`] available for use, a new connection will be established
/// and it could be discarded after use.
pub(crate) struct ClientManager {
    // A hash map guarded by a read-write mutex. Pretty much everytime we interact with the pool a
    // write will be executed anyways, there is no interaction that we only read the map.
    // The key used here is the destination address.
    pool: RwLock<HashMap<String, VecDeque<TcpClient>>>,
}

/// A client wrapper around the established TCP connection. This represents only a single
/// connection, so multiple clients can mutually exists with the same destination.
///
/// The only exposed API to interact with the underlying connection is a `send` method, which
/// is used to send data to the socket that the structure is connected.
struct TcpClient {
    // The underlying TCP connection. In the current context, this is used only to send messages.
    connection: TcpConnection,
}

impl ClientManager {
    /// Creates a new [`ClientManager`] with an empty connection pool.
    pub(crate) fn new() -> Self {
        ClientManager {
            pool: RwLock::new(HashMap::new()),
        }
    }

    /// Writes the data to the given destination.
    ///
    /// The data must be serializable so it can be written to the socket as bytes. At this point we
    /// are also considering that the destination is a correct address to the other peer, and we do
    /// not execute any validation anymore.
    ///
    /// This method works by acquiring a [`TcpClient`], writing data using the client and then
    /// putting the client back into the connection pool. The process to acquire a new client
    /// consist of multiple steps, the first one is to verify the connection pool if there is a
    /// client and removing the first element from the [`VecDequeue`] pool. If no entry is found in
    /// the connection pool, a new TCP connection will be established and a [`TcpClient`] will be
    /// created.
    ///
    /// After using the client to write the message, we need to put it back into the pool so it can
    /// be used later. If an entry already exists, we verify if already exists 10 connections,
    /// if this limit is reached the client will be discarded, otherwise we save the client into
    /// the connection pool.
    pub(crate) async fn write_to(
        &mut self,
        destination: &str,
        data: impl Into<String>,
    ) -> crate::Result<()> {
        let destination = destination.to_string();
        // First we acquire a client, this can used a client present in the pool or a new client
        // could be created dynamically.
        let mut client = self.resolve_client(&destination).await?;
        client.write(data).await?;

        // We put the client back into the connection pool so it can be used later. If the limit
        // is reached for a specific destination, then the client will be discard, so the method
        // must take ownership of the client structure.
        self.put_back(destination, client).await;
        Ok(())
    }

    /// Resolves a [`TcpClient`] that is connected to the given address.
    ///
    /// This method will try to acquire the client from the connection pool, and if not found it
    /// will request so a new client is created. If a new client is created dynamically then the
    /// a new TCP connection must be established first.
    ///
    /// # Errors
    ///
    /// This method will fail if no connection was found on the pool and then we try to create a
    /// new client and during this process of creation the TCP connection can not be established.
    async fn resolve_client(&mut self, address: &String) -> crate::Result<TcpClient> {
        let client = match self.try_acquire(address).await {
            Some(c) => c,
            None => self.create_client(address).await?,
        };
        Ok(client)
    }

    /// This method tries to acquire a [`TcpClient`] from the connection pool.
    ///
    /// Since we need to acquire a client from the pool of connections, we need to hold a write
    /// lock to the structure holding all connections. When a mutable reference exists, all other
    /// references should no exists, so is possible that this method can be a point of contention.
    async fn try_acquire(&mut self, address: &String) -> Option<TcpClient> {
        let mut pool = self.pool.write().await;
        let existent = (*pool).get_mut(address);

        match existent {
            // We should remove the connection from the list so this client is used only by a
            // single place at any time.
            Some(connections) => connections.pop_front(),
            None => None,
        }
    }

    /// Creates a new [`TcpClient`] connected to the given address.
    ///
    /// This method will create a whole new [`TcpClient`], meaning that a TCP connection must be
    /// established before creating the structure itself.
    ///
    /// # Errors
    ///
    /// This method can fail if we are not able to establish the connection with the destination.
    async fn create_client<T: ToSocketAddrs>(&mut self, address: T) -> crate::Result<TcpClient> {
        let socket = TcpStream::connect(address).await?;
        Ok(TcpClient::new(socket))
    }

    /// Insert the client back into the connection pool so it can be used later.
    ///
    /// This method will save the [`TcpClient`] into the connection pool if the limit of connection
    /// was not reached with the given destination address. The limit of connections for any
    /// destination if 10 connections, which is not configurable.
    ///
    /// Since we will write back into the connection pool, we need to hold the write lock to the
    /// pool structure. This method could also be a retention point.
    async fn put_back(&mut self, address: String, client: TcpClient) {
        let mut pool = self.pool.write().await;
        match (*pool).get_mut(&address) {
            Some(connections) => {
                // If the maximum limit was not reached we will insert the client into the pool of
                // connections. Since the `connections` variable is a mutable reference to the
                // vector, we only need to push_back the client and do not need to re-insert to the
                // pool structure.
                if connections.len() < 10 {
                    connections.push_back(client);
                }
            }
            None => {
                // This is the first time we are saving a connection to be used later, so a new
                // array must be created and the entry into the pool structure must be created.
                let mut connections = VecDeque::new();
                connections.push_back(client);
                (*pool).insert(address, connections);
            }
        };
    }
}

impl TcpClient {
    /// Create a new [`TcpClient`] around the given socket structure. The connection is already
    /// established, now we only create a wrapper around it.
    fn new(socket: TcpStream) -> Self {
        TcpClient {
            connection: TcpConnection::new_writer(socket),
        }
    }

    /// Send the data to the destination in which the socket is connected. The data must be
    /// convertible to a [`String`] so is possible to write it to the socket.
    ///
    /// # Errors
    ///
    /// This method can fail if is not possible to write or flush the data to the underlying
    /// socket.
    async fn write(&mut self, data: impl Into<String>) -> crate::Result<()> {
        self.connection.write(data.into()).await
    }
}

/// Parse a [`std::io::Error`] that can happen while writing data to the expected [`crate::Error`].
impl From<std::io::Error> for crate::Error {
    fn from(e: std::io::Error) -> Self {
        crate::Error::SocketError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::client::ClientManager;
    use std::net::SocketAddr;
    use tokio::task::JoinHandle;

    /// This test will create the server in a random available port and then proceed to send
    /// 15 messages. This is enough to exceed the connection pool size for a single destination.
    #[tokio::test]
    async fn send_multiple_messages() {
        let (addr, _) = create_server().await;
        let mut client = ClientManager::new();

        for i in 1..15 {
            let result = client.write_to(&addr.to_string(), i.to_string()).await;
            assert!(result.is_ok());
        }
    }

    async fn create_server() -> (SocketAddr, JoinHandle<()>) {
        let mut server = crate::create_rx(0).await.unwrap();
        let address = server.local_address().unwrap();

        let handle = tokio::spawn(async move {
            assert!(server.poll(|_| async {}).await.is_ok());
        });

        (address, handle)
    }
}
