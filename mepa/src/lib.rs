//! A minimal project that offers a transport primitive backed by TCP.
//!
//! The main purpose of this project is to abstract all complexity and offer a high level
//! communication primitive to be used by other projects. Here we are looking forward to offer
//! a simple TCP communication between a pair of peers, nothing too fancy.
//!
//! First things first, everything here is heavily based on the mini-redis implementation by the
//! Tokio team, so, even when we simplify the components keep in mind that we are following much
//! of an already existent project in here.
//!
//! We are following a simple architecture to handle received messages and messages to be sent.
//! The diagram bellow is a high level view of how the project is organized.
//!
//!             +---------------+                 +-------------------+
//!             |               |                 |                   |
//!             |   Transport   |<-----Stream-----+   TcpConnection   |
//!             |               |                 |                   |
//!             +------+--------+                 +-------------------+
//!                    |                                    ^
//!                    |                                    |
//!                  Write        +------------+          Send
//!                    |          |            |            |
//!                    +--------->|   Buffer   +------------+
//!                               |            |
//!                               +------------+
//!
//! The will create a TCP socket that will be handled by the `TCPConnection` structure, this
//! structure will be responsible for writing messages to the socket and notifying the
//! [`crate::Transport`] of any received message.
//!
//! The only exposed part of the project is the [`crate::Transport`] structure, which is the
//! structure that the library users will interact with. For each message sent by the users,
//! since a TCP socket is used, there is no concurrent writes, so the messages will be buffered
//! to be eventually sent by the `TCPConnection` structure.
//!
//! Since we are learning, we will use [`tokio`] all around, along with some other crates to help,
//! which are the [`tokio-stream`], [`async-stream`] and [`futures-util`].

pub use crate::transport::create;
pub use crate::transport::Receiver;
pub use crate::transport::Sender;
mod client;
mod server;
mod shutdown;
mod tcp_connection;
mod transport;

/// The possible errors that can occur when using the current library. Each error will be
/// associated with a description so the user can known what went wrong, another metadata
/// can also be included when is required to explain the context.
#[derive(Debug)]
pub enum Error {
    ConnectionReset,
    SocketError(String),
}

/// A convenience type that will be used in all operations for the current library.
pub type Result<T> = std::result::Result<T, Error>;
