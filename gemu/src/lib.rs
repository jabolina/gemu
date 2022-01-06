mod transport;

enum Error {}

type Result<T> = std::result::Result<T, Error>;
