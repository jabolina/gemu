use crate::tcp_connection::TcpConnection;
use async_stream::try_stream;
use futures_util::Stream;
use std::io::Error;
use tokio::net::{TcpStream, ToSocketAddrs};

pub struct TcpClient {
    connection: TcpConnection,
}

impl TcpClient {
    fn new(socket: TcpStream) -> Self {
        TcpClient {
            connection: TcpConnection::new(socket),
        }
    }

    pub(crate) async fn listen<'a>(&'a mut self) -> impl Stream<Item = crate::Result<String>> + 'a {
        try_stream! {
            while let Some(data) = self.connection.read().await? {
                yield data
            }
        }
    }
}

pub(crate) async fn connect<T: ToSocketAddrs>(address: T) -> crate::Result<TcpClient> {
    let socket = TcpStream::connect(address).await?;
    Ok(TcpClient::new(socket))
}

impl From<std::io::Error> for crate::Error {
    fn from(e: Error) -> Self {
        crate::Error::SocketError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::client::connect;
    use futures_util::{pin_mut, StreamExt};

    #[tokio::test]
    async fn create_and_and_receive() {
        let st_peer = connect("127.0.0.1:6379").await;
        let nd_peer = connect("localhost:6378").await;

        if let Err(e) = st_peer {
            eprintln!("{:?}", e);
            return;
        }
        assert!(st_peer.is_ok());
        assert!(nd_peer.is_ok());

        let mut st_peer = st_peer.unwrap();
        let mut nd_peer = nd_peer.unwrap();

        let handler = tokio::spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async move {
                let stream = st_peer.listen().await;
                pin_mut!(stream);
                while let Some(v) = stream.next().await {
                    assert!(v.is_ok());
                    let content = v.unwrap();
                    assert_eq!(content, "hello");
                    break;
                }
            })
            .await
        })
        .await;
    }
}
