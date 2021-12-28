//! Provides a simple TCP connection wrapper.
//!
//! Using this structure the [`crate::Transport`] structure will be able to send and listen for
//! incoming messages. This structure is composed by a client and a server, and is responsible to
//! handle both structures in any means necessary.
use bytes::{Buf, BytesMut};
use std::io::Cursor;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

/// Represent a single TCP connection, this connection can be used to read and write data.
pub(crate) struct TcpConnection {
    // The underlying socket itself
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
}

impl TcpConnection {
    pub(crate) fn new(socket: TcpStream) -> Self {
        TcpConnection {
            stream: BufWriter::new(socket),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
    }

    fn try_read_buffer(&mut self) -> crate::Result<Option<String>> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        match std::str::from_utf8(&self.buffer.to_vec()) {
            Ok(s) => {
                self.buffer.advance(s.len());
                Ok(Some(String::from(s)))
            }
            Err(..) => Ok(None),
        }
    }

    // TODO: handle reading more carefully
    pub(crate) async fn read(&mut self) -> crate::Result<Option<String>> {
        loop {
            if let Some(v) = self.try_read_buffer()? {
                return Ok(Some(v));
            }

            if 0 == self.stream.read_buf(&mut self.buffer).await? {
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err(crate::Error::ConnectionReset)
                };
            }
        }
    }

    pub(crate) async fn write(&mut self, data: String) -> crate::Result<()> {
        self.stream.write_all(data.as_bytes()).await?;
        Ok(())
    }
}
