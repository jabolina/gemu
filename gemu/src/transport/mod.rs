use std::future::Future;

pub(crate) mod group_communication;
mod primitive;
pub(crate) mod process_communication;

pub(crate) trait AsyncTrait<'a>: Send + Sync {
    type Future: Future<Output = TransportResult<()>> + Send + 'a;
}

pub(crate) trait Listener<'a>: AsyncTrait<'a> {
    fn handle(&self, data: String) -> Self::Future;
}

pub(crate) trait Sender<'a>: AsyncTrait<'a> {
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future;
}

#[derive(Debug)]
pub(crate) enum TransportError {
    WriteError(String),
    ShutdownError(String),
    GroupError(String),
}

type TransportResult<T> = std::result::Result<T, TransportError>;
