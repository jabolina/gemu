pub mod algorithm;
mod internal;
mod transport;

pub use algorithm::ConflictRelationship;
pub use algorithm::GenericMulticast;
pub use algorithm::GenericMulticastConfiguration;
pub use algorithm::Oracle;

#[derive(Debug)]
pub enum Error {
    TransportError(String),
}

type Result<T> = std::result::Result<T, Error>;
