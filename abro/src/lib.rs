//! A minimal project that offers an atomic broadcast primitive.
//!
//! The main purpose of this project is to create a communication primitive that offers
//! all the requirements for atomic broadcast. This could be used by other projects that
//! requires a reliable totally ordered communication primitive. This project _will not_ implement
//! a consensus algorithm, instead we will use the etcd implementation of the raft protocol.
//!
//! # How does it work?
//!
//! Since we are backed by etcd, this means that we have a distributed KV to work with. So, starting
//! a new peer using the current library we will connect to a running etcd server and we will be
//! a participant of a specified partition throughout the complete lifetime of the application.
//! This means that after a peer starts, it will bind itself to a single partition and will remain
//! attached to it, to change from one partition to another the peer should be destroyed and
//! a new one must be created.
//!
//! After the peer is connected to a specific partition, broadcasting a message means that the
//! contents of the message will be written to the KV store in the partition the peer belongs to.
//! Receiving a message means that the peer will listen for changes that happen in the partition
//! in which he is bound.
//!
//! # Requirements
//!
//! Since this library is backed by etcd, an etcd server must be available and we must be able
//! to connect to it. At this point we are dealing with only a single etcd server address, but
//! I suppose that is possible to use a cluster of etcd servers in the future as well.
//!
//! # Guarantees
//!
//! We are offering a complex primitive that is working through a high level send/receive API.
//! This primitive is the atomic broadcast, the protocol itself is implemented by the etcd
//! and we are only connecting and abstracting some of the possible boilerplate. So, since we
//! are backed by the Raft protocol, we offer the same guarantees of the atomic broadcast:
//!
//! - Validity - if a correct process _broadcast_ a message `m`, then it eventually _deliver_ `m`.
//! - Agreement - if a correct process _deliver_ a message `m`, then all correct processes eventually
//!               _deliver_ `m`.
//! - Integrity - for any message `m`, every correct process _deliver_ `m` at most once, and only if
//!               `m` was previously _broadcast_ by some process.
//! - Total Order - if correct processes `p` and `q` both _deliver_ messages `m` and `n`, then `p`
//!                 _deliver_ `m` before `n`, if and only if, `q` _deliver_ `m` before `n`.
//!
//! Here we are being a little conservative and referring only to _correct processes_, since I am
//! not sure if the etcd implementation is uniform or not. If the etcd implement does implement an
//! uniform atomic broadcast, this means that we also should be able to offer the same uniform
//! guarantees.

pub use crate::transport::Transport;
pub use crate::transport::TransportConfiguration;
pub mod transport;
mod wrapper;

/// The possible errors that can occur when using the current library. Each error will be
/// associated with a description so the user can known what went wrong, another metadata
/// can also be included when is required to explain the context.
///
/// An enum will be used in order to avoid boxing over a dynamic trait.
#[derive(Debug)]
pub enum Error {
    /// This error type identify that an error occurred while sending a message. A string
    /// is carried along so is possible to identify what happened during the write.
    SendError(String),

    /// This is the broad generic error, used when we are not able to perfectly identify
    /// what is the underlying cause.
    InternalError(String),
}

/// A convenience type that will be used in all operations for the current library.
pub type Result<T> = std::result::Result<T, Error>;
