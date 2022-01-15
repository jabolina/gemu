pub mod algorithm;
mod internal;
mod transport;

enum Error {
    TransportError(String),
}

type Result<T> = std::result::Result<T, Error>;
