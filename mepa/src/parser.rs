//! This is the parser used to write and read data from the underlying stream. We created a
//! dedicated file for this so we can change the data parsing without affecting any of TCP
//! connection structure.
//!
//! At the moment we are using a length-prefix framing approach, so, for each data written to the
//! TCP socket, an additional 2 bytes are reserved to the data length, and then the data itself.
//! This method is used simply because it is easier to implement and I think that the 2 bytes
//! overhead is better than using delimiters and escaping the data.
//!
//! The implementation here is also pretty straightforward, we expose two methods [`write_buffer`]
//! and [`read_buffer`]. The write will receive the buffer and the data, will write the size and
//! the data itself, and the read will start by reading the first 2 bytes and then reading a slice
//! containing the complete data.
use std::io::Cursor;
use std::string::FromUtf8Error;

use bytes::Buf;
use std::marker;
use tokio::io;

/// Defines the specific errors that can happen while parsing the buffer data.
#[derive(Debug)]
pub enum ParseError {
    /// The data on the buffer is incomplete.
    Incomplete,

    /// Any other type of error occurred.
    Other(String),
}

// Just a convenience to be used only here internally, since we have a specific error type.
type ParseResult<T> = std::result::Result<T, ParseError>;

/// Write the data to the given buffer.
///
/// This method will write the data to the given buffer. Here we will follow the length-prefix
/// framing method. We will use 2 bytes to define the data size and then the data itself.
pub(crate) async fn write_buffer<T>(buffer: &mut T, data: String) -> ParseResult<()>
where
    T: io::AsyncWriteExt + marker::Unpin,
{
    // First write the data length as u16 to use the first 2 bytes.
    buffer.write_u16(data.len() as u16).await?;
    buffer.write_all(data.as_bytes()).await?;
    Ok(())
}

/// Read data from the buffer.
///
/// Using a buffer wrapped by a [`Cursor`] helper we will try to read the data. We are using a
/// length-prefix framing, so, the first step is to read the first 2 bytes from the buffer so we
/// know if the buffer contains the complete data.
///
/// Using the [`read_u16`] we retrieve the first 2 bytes that contains the length information, then
/// we proceed to [`extract`] the data itself and transform the u8 slice into a vec so the data can
/// be transformed into a String and returned.
///
/// # Errors
///
/// This method will fail with [`ParseError::Incomplete`] if there is no data or the data is not
/// complete on the buffer yet. If another error occur a [`ParserError::Other`] will be returned.
pub(crate) fn read_buffer(src: &mut Cursor<&[u8]>) -> ParseResult<String> {
    match read_u16(src) {
        Err(e) => Err(e),
        Ok(size) => {
            let data = extract(size as usize, src)?.to_vec();
            Ok(String::from_utf8(data)?)
        }
    }
}

/// Read 2 bytes from the buffer if there is available data. If the buffer has no data yet then a
/// [`ParserError::Incomplete`] will be returned.
fn read_u16(src: &mut Cursor<&[u8]>) -> ParseResult<u16> {
    if !src.has_remaining() {
        return Err(ParseError::Incomplete);
    }

    Ok(src.get_u16())
}

/// Try to extract _N_ bytes from the buffer, where _N_ is the size parameter. Calling this method
/// and successfully extracting the bytes will cause the cursor position to move.
///
/// # Errors
///
/// If the buffer does not have enough data a [`FrameError::Incomplete`] will be returned.
fn extract<'a>(size: usize, src: &mut Cursor<&'a [u8]>) -> ParseResult<&'a [u8]> {
    let start = src.position() as usize;
    let end = src.get_ref().len();

    // Does the buffer have enough data that completes the size requirement?
    if size > (end - start) {
        return Err(ParseError::Incomplete);
    }

    // We move the cursor position and read the data.
    src.set_position((start + size) as u64);
    Ok(&src.get_ref()[start..(start + size)])
}

/// Transform the specific parse errors to the usual [`crate::Error`].
impl From<ParseError> for crate::Error {
    fn from(e: ParseError) -> Self {
        crate::Error::FrameError(e)
    }
}

/// Transform the error that happen while converting bytes to String to a parser error.
impl From<FromUtf8Error> for ParseError {
    fn from(e: FromUtf8Error) -> Self {
        ParseError::Other(e.to_string())
    }
}

/// Parse a [`std::io::Error`] to a parser error.
impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bytes::{BufMut, BytesMut};

    use crate::parser::read_buffer;

    #[test]
    fn should_parse_data_correctly() {
        let data = "hello";
        let size = data.len() as u16;

        let mut buf = BytesMut::with_capacity((size + 2) as usize);
        buf.put_u16(size);
        buf.put_slice(data.as_bytes());

        let mut cursor = Cursor::new(&buf[..]);

        let data = read_buffer(&mut cursor);
        assert!(data.is_ok());
        let data = data.unwrap();
        assert_eq!(data, String::from(data.clone()));
    }

    #[test]
    fn more_data_than_buffer_capacity() {
        let data = "hello, world, with a data little more large";
        let mut buf = BytesMut::with_capacity(5);
        buf.put_u16(data.len() as u16);
        buf.put_slice(data.as_bytes());

        let mut cursor = Cursor::new(&buf[..]);

        let data = read_buffer(&mut cursor);
        assert!(data.is_ok());
        let data = data.unwrap();
        assert_eq!(data, String::from(data.clone()));
    }
}
