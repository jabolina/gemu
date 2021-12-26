//! Here is implemented the transport primitive itself.
//!
//! This is the structure that should be used by the users, so any complexity can be avoided
//! if using a direct connection into etcd.

/// This structure holds the configuration required by the current project.
///
/// This is used in order to connect to the etcd server and bind the current peer to
/// the given partition.
pub struct TransportConfiguration {
    host: String,
    port: String,
    partition: String,
}

pub struct Transport {}
