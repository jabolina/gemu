use crate::parser;
use bytes::{Buf, BytesMut};
use std::io::Cursor;
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

    // Used to help when we read data from the stream.
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
    /// will be used to read data from the underlying stream.
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
    fn try_read_buffer(&mut self) -> crate::Result<Option<String>> {
        // Here we are creating a cursor because this wrapper can aid while parsing the underlying
        // data with some convenience methods.
        let mut buf = Cursor::new(&self.buffer[..]);
        match parser::read_buffer(&mut buf) {
            Ok(data) => {
                // We were able to consume the data using the buffer within the cursor helper, but
                // in our local buffer the data was not consumed yet, so we need to advance the
                // position and the data will be declared as consumed.
                let len = buf.position() as usize;
                self.buffer.advance(len);
                Ok(Some(data))
            }

            // This means that we were not able to retrieve the complete data from the buffer yet,
            // so we just return an Ok saying that None data was found.
            Err(parser::ParseError::Incomplete) => Ok(None),

            // Something went wrong while reading the buffer.
            Err(e) => Err(e.into()),
        }
    }

    /// Read data from the underlying socket connection.
    ///
    /// This method will return only if data is found, if no bytes are returned from the read or
    /// if the connection is closed by the peer.
    ///
    /// # Errors
    ///
    /// The returned [`crate:Error`] is returned when the connection is closed by the peer.
    async fn read(&mut self) -> crate::Result<Option<String>> {
        loop {
            if let Some(v) = self.try_read_buffer()? {
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

    /// Write the data into the stream. Given a serialized data, the complete value will be written
    /// to the socket and flushed. We are using the dedicated parser to write the data.
    ///
    /// # Errors
    ///
    /// This can fail for any I/O error that can happen either while writing or flushing the data.
    async fn write(&mut self, data: String) -> crate::Result<()> {
        parser::write_buffer(&mut self.stream, data).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
