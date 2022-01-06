use std::future::Future;

pub(crate) mod group_communication;
pub(crate) mod process_communication;

trait Listener {
    type Future: Future<Output = TransportResult<()>>;

    fn handle(&self, data: String) -> Self::Future;
}

#[derive(Debug)]
enum TransportError {
    WriteError(String),
    ShutdownError(String),
    GroupError(String),
}

type TransportResult<T> = std::result::Result<T, TransportError>;
