use bytes::{Buf, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;

/// Represent a single TCP connection.
///
/// We can have only types dedicated to some type of operation, since each TCP connection type will
/// take ownership over the [`TcpStream`] we can not use both [`BufReader`] and [`BufWriter`] at
/// the same time, so we created dedicated connection types.
pub(crate) enum TcpConnection {
    /// A TCP connection type that is dedicated to write data to the given stream.
    Writer(TcpConnectionWriter),

    /// A TCP connection type that is dedicated to reading data from the given stream.
    Reader(TcpConnectionReader),
}

pub(crate) struct TcpConnectionReader {
    // The underlying socket structure decorated with a buf reader. This will automatically buffer
    // data from the TCP socket.
    stream: BufReader<TcpStream>,

    // Used to help when we read data from the stream. Data will be read in chunks the size of this
    // buffer, so at this time we have no conception of frame whatsoever, so is possible that is
    // split between multiple chunks.
    buffer: BytesMut,
}

pub(crate) struct TcpConnectionWriter {
    // The underlying socket itself, it is wrapped around a buffer. This avoids executing too many
    // syscall, although that would not be problem in our case where we write the complete data
    // to the stream and flush it.
    stream: BufWriter<TcpStream>,
}

impl TcpConnection {
    /// Creates a new connection that can only read data from the stream.
    pub(crate) fn new_reader(socket: TcpStream) -> TcpConnection {
        TcpConnection::Reader(TcpConnectionReader::new(socket))
    }

    /// Creates a new connection that can only write data to the stream.
    pub(crate) fn new_writer(socket: TcpStream) -> TcpConnection {
        TcpConnection::Writer(TcpConnectionWriter::new(socket))
    }
}

impl TcpConnection {
    /// This is an abstraction that will pipe the request if the connection type is correct.
    ///
    /// # Errors
    ///
    /// This will return a [`crate::Error`] if calling `read` on a `TcpConnection::Writer`. Other
    /// errors come from the read operation itself.
    pub(crate) async fn read(&mut self) -> crate::Result<Option<String>> {
        match self {
            TcpConnection::Writer(..) => {
                Err(crate::Error::ConnectionType(String::from("not a reader")))
            }
            TcpConnection::Reader(r) => r.read().await,
        }
    }

    /// This is just an abstraction that will call the method if the connection has the right type.
    ///
    /// # Errors
    ///
    /// This will return a [`crate::Error`] if calling the `write` on a `TcpConnection::Reader`.
    /// Any other error is from the write operation itself.
    pub(crate) async fn write(&mut self, data: String) -> crate::Result<()> {
        match self {
            TcpConnection::Reader(..) => {
                Err(crate::Error::ConnectionType(String::from("not a writer")))
            }
            TcpConnection::Writer(w) => w.write(data).await,
        }
    }
}

impl TcpConnectionReader {
    /// Create a new instance of the socket wrapper. This will start with a buffer of 8Kb size that
    /// will be used to read data from the underlying stream, this means that varying on the
    /// package size, it could be split between multiple chunks.
    fn new(socket: TcpStream) -> Self {
        Self {
            stream: BufReader::new(socket),
            buffer: BytesMut::with_capacity(8 * 1024),
        }
    }

    /// Try to read data that is already buffered.
    ///
    /// If the buffer is not empty, we will try to transform the received bytes into a UTF-8
    /// String representation. If the received bytes are not an accepted UTF-8 sequence we will
    /// discard the values by advancing the buffer to "consume" the invalid sequence and returning
    /// `None`, since we were not able to parse the data.
    fn try_read_buffer(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }

        // Read the buffered data and advance the buffer.
        match std::str::from_utf8(&self.buffer.to_vec()) {
            Ok(s) => {
                self.buffer.advance(s.len());
                Some(String::from(s))
            }
            Err(..) => {
                // Is this the correct workaround to discard the invalid UTF-8 sequence?
                self.buffer.advance(self.buffer.len());
                None
            }
        }
    }

    /// Read data from the underlying socket connection.
    ///
    /// This method will return only if data is found, if no bytes are returned from the read or
    /// if the connection is closed by the peer. Since the data is buffered, the data is read in
    /// chunks.
    ///
    /// # Errors
    ///
    /// The returned [`crate:Error`] is returned when the connection is closed by the peer.
    // TODO: handle reading more carefully, create a frame abstraction?
    async fn read(&mut self) -> crate::Result<Option<String>> {
        loop {
            if let Some(v) = self.try_read_buffer() {
                return Ok(Some(v));
            }

            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err(crate::Error::ConnectionReset)
                };
            }
        }
    }
}

impl TcpConnectionWriter {
    /// Create a new instance of the socket wrapper that is dedicated to write operations.
    fn new(socket: TcpStream) -> Self {
        Self {
            stream: BufWriter::new(socket),
        }
    }

    /// Write the data into the stream.
    ///
    /// Given a serialized data, the complete value will be written to the socket and flushed. This
    /// is probably doing a single syscall?
    ///
    /// # Errors
    ///
    /// This can fail for any I/O error that can happen either while writing or flushing the data.
    async fn write(&mut self, data: String) -> crate::Result<()> {
        self.stream.write_all(data.as_bytes()).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

/// Parse a [`std::io::Error`] that can happen while writing data to the expected [`crate::Error`].
impl From<std::io::Error> for crate::Error {
    fn from(e: std::io::Error) -> Self {
        crate::Error::SocketError(e.to_string())
    }
}
