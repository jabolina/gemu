//! In this module is implemented all the communication primitives required.
//!
//! Here is implemented the simple process communication, which is backed by the implementation in
//! the [`mepa`] crate we created. The group communication is also defined here, which is backed by
//! the [`abro`] create we also created.
//!
//! Since both primitives are abstracted pretty much the same way, there is a generic [`Transport`]
//! structure which wrap the concrete implementation. So some code duplication could be avoided,
//! but, if I am being honest, maybe duplicating code could lead to a simpler implementation.
//!
//! Here in the root is defined some convenience types and traits that are used to implement the
//! [`Transport`] primitive.
use std::future::Future;

use crate::transport::primitive::Transport;
pub(crate) mod group_communication;
mod primitive;
pub(crate) mod process_communication;

/// A basic trait to be used to define asynchronous methods.
///
/// Since `async` is not valid in traits, this can be used to define a synchronous method that will
/// return a future to be completed. This trait is [`Send`] and [`Sync`], the associated type must
/// be defined every time, since is not possible to define default associated types yet. Most of
/// the time it will be a pinned future which is boxed. If for some reason, we implement our custom
/// [`Future`], it could be used with this trait.
///
/// [`Future`]: std::future::Future
pub(crate) trait AsyncTrait<'a, T>: Send + Sync {
    type Future: Future<Output = T> + Send + 'a;
}

/// The [`Listener`] trait, this will handle incoming messages.
///
/// The listener is an asynchronous trait, since the `handle` method is executed asynchronously.
/// This will require an implementation for the concrete implementer, but the approach using
/// pin + box + future will work just fine, but it will need some boilerplate.
pub(crate) trait Listener<'a>: AsyncTrait<'a, ()> {
    /// Handle incoming messages.
    ///
    /// Structures that implements the listener are responsible to handling all incoming messages.
    /// The data is serialized as a [`String`], so is up to the implementer to deserialize into the
    /// expected format.
    fn handle(&self, data: String) -> Self::Future;
}

/// Defines the [`Sender`] trait.
///
/// This should be implemented by the concrete transport primitives to send messages. This is
/// required so we can use the generic transport wrapper more easily without boxing any concrete
/// implementation.
pub(crate) trait Sender<'a>: AsyncTrait<'a, TransportResult<()>> {
    /// Sends a message to a specific destination.
    ///
    /// This will be piped to a concrete implementation, so all parameters must be able to live as
    /// long as the [`Sender`] concrete implementation.
    fn send(
        &'a mut self,
        destination: &'a str,
        data: impl Into<String> + Send + 'a,
    ) -> Self::Future;
}

/// The possible errors that can occur while using the current transport implementation.
#[derive(Debug)]
pub(crate) enum TransportError {
    /// Used when something went wrong while sending a message.
    SendingError(String),

    /// Used when failed to shutdown the transport primitive.
    ShutdownError(String),

    /// Used if an error happened during the transport setup.
    SetupError(String),
}

/// Transform the specific [`TransportError`] the the crate level error.
impl From<TransportError> for crate::Error {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::SendingError(v)
            | TransportError::ShutdownError(v)
            | TransportError::SetupError(v) => crate::Error::TransportError(v),
        }
    }
}

/// A convenience result type for transport operations.
type TransportResult<T> = std::result::Result<T, TransportError>;
