pub use configuration::ConfigurationBuilder;
pub use configuration::GenericMulticastConfiguration;
pub use writer::GenericMulticast;

pub(crate) use message::Message;
pub(crate) use message::MessageStatus;

mod configuration;
mod handler;
mod message;
mod writer;

pub trait ConflictRelationship<V>: Send + Sync {
    fn conflict(&self, lhs: &V, rhs: &V) -> bool;
}

pub trait Oracle: Send + Sync {
    fn identify(&self, partition: String) -> Vec<String>;
}

#[derive(Debug)]
pub enum AlgorithmError {
    ConfigurationMissing(String),
}

impl From<AlgorithmError> for crate::Error {
    fn from(_: AlgorithmError) -> Self {
        todo!()
    }
}

pub(crate) type AlgorithmResult<T> = std::result::Result<T, AlgorithmError>;
