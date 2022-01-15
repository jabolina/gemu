pub use writer::GenericMulticast;

pub(crate) use message::Message;

mod handler;
mod message;
mod writer;

pub trait ConflictRelationship<V> {
    fn conflict(&self, lhs: &V, rhs: &V) -> bool;
}

#[derive(Debug)]
pub enum AlgorithmError {}

pub(crate) type AlgorithmResult<T> = std::result::Result<T, AlgorithmError>;
