mod handler;
mod message;
mod writer;

pub use writer::GenericMulticast;

#[derive(Debug)]
pub enum AlgorithmError {}

pub(crate) type AlgorithmResult<T> = std::result::Result<T, AlgorithmError>;
