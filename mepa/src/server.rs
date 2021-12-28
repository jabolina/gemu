use crate::shutdown::Shutdown;
use crate::tcp_connection::TcpConnection;
use std::net::ToSocketAddrs;
use tokio::net::TcpStream;
use tokio::{select, sync};

pub struct TcpServer {
    listener: tokio::net::TcpListener,
    shutdown: sync::broadcast::Sender<()>,
}

struct Handler {
    connection: TcpConnection,
    shutdown: Shutdown,
}

/// Through this step we are able to create a new [`tokio::net::TcpListener`] locally into the
/// given port.
async fn bind(port: usize) -> crate::Result<TcpServer> {
    let listener = tokio::net::TcpListener::bind(&format!("127.0.0.1:{}", port)).await?;
    let (tx, _) = sync::broadcast::channel(1);
    Ok(TcpServer {
        listener,
        shutdown: tx,
    })
}

pub async fn start(port: usize) -> crate::Result<()> {
    let mut server = bind(port).await?;

    select! {
        res = server.poll() => {
            if let Err(e) = res {
                eprintln!("Failed: {:?}", e)
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("Shutdown")
        }
    }

    Ok(())
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
    async fn poll(&mut self) -> crate::Result<()> {
        println!("Start to listen for connections");
        loop {
            // Tries to accept new connections. Because of the error handling that happens inside
            // the method is possible that we fail and stop up to the point of closing the server.
            let socket = self.accept().await?;
            let mut handler = Handler {
                connection: TcpConnection::new(socket),
                shutdown: Shutdown::new(self.shutdown.subscribe()),
            };
            tokio::spawn(async move {
                if let Err(e) = handler.process().await {
                    eprintln!("Connection error: {:?}", e);
                }
                println!("Connection closed");
            });
        }
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
    async fn process(&mut self) -> crate::Result<()> {
        println!("Accepted connection");
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

            println!("DATA: {}", data);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::server::start;

    #[tokio::test]
    async fn should_start_and_listen() {
        let result = start(8080).await;
        assert!(result.is_ok());
    }
}
