use crate::tcp_connection::TcpConnection;
use std::collections::{HashMap, VecDeque};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::RwLock;

pub(crate) struct ClientManager {
    pool: RwLock<HashMap<String, VecDeque<TcpClient>>>,
}

struct TcpClient {
    connection: TcpConnection,
}

impl ClientManager {
    pub(crate) fn new() -> Self {
        ClientManager {
            pool: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) async fn write_to(
        &mut self,
        destination: &str,
        data: impl ToString,
    ) -> crate::Result<()> {
        let destination = destination.to_string();
        let mut client = self.resolve_client(&destination).await?;
        client.write(data).await?;
        self.put_back(destination, client).await?;
        Ok(())
    }

    async fn resolve_client(&mut self, address: &String) -> crate::Result<TcpClient> {
        let client = match self.try_acquire(address).await {
            Some(c) => c,
            None => self.create_client(address).await?,
        };
        Ok(client)
    }

    async fn try_acquire(&mut self, addr: &String) -> Option<TcpClient> {
        let mut pool = self.pool.write().await;
        let existent: Option<&mut VecDeque<TcpClient>> = (*pool).get_mut(addr);
        if let Some(connections) = existent {
            return connections.pop_front();
        }

        None
    }

    async fn create_client<T: ToSocketAddrs>(&mut self, addr: T) -> crate::Result<TcpClient> {
        let socket = TcpStream::connect(addr).await?;
        Ok(TcpClient::new(socket))
    }

    async fn put_back(&mut self, address: String, client: TcpClient) -> crate::Result<()> {
        let mut pool = self.pool.write().await;
        match (*pool).get_mut(&address) {
            Some(v) => {
                if v.len() < 10 {
                    v.push_back(client);
                }
            }
            None => {
                let mut v = VecDeque::new();
                v.push_back(client);
                (*pool).insert(address, v);
            }
        };

        Ok(())
    }
}

impl TcpClient {
    fn new(socket: TcpStream) -> Self {
        TcpClient {
            connection: TcpConnection::new(socket),
        }
    }

    async fn write(&mut self, data: impl ToString) -> crate::Result<()> {
        self.connection.write(data.to_string()).await
    }
}

#[cfg(test)]
mod tests {
    use crate::client::{ClientManager, TcpClient};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn bind_and_send() {
        let mut manager = ClientManager::new();
        let messages = vec!["hello", "how are", "you", "????"];
        for message in messages {
            manager
                .write_to("localhost:8080", message)
                .await
                .expect("Should be ok!");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}
