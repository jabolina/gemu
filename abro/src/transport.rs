//! Here is implemented the transport primitive itself.
//!
//! This is the structure that should be used by the users, so any complexity can be avoided
//! if using a direct connection into etcd. We will offer this transport primitive with a simple
//! 'send' and 'receive' API, being simple to avoid any complexity that start growing from
//! the start of the project.

use crate::wrapper::{connect, EtcdWrapper};
use futures_util::{pin_mut, StreamExt};
use std::future::Future;

/// This structure holds the configuration required by the current project.
///
/// This is used in order to connect to the etcd server and bind the current peer to the given
/// partition. At this moment we are dealing with only a single etcd server, it could fun to
/// extend this structure so we can start using multiple etcd servers at the same time.
pub struct TransportConfiguration {
    /// Identify the address of the single etcd server that we will connect into.
    /// This address will have the format of `[host]:[port]` and will be built directly by the
    /// [`TransportConfigurationBuilder`] when the [`TransportConfigurationBuilder::build()`]
    /// method is called.
    address: String,

    /// Identify the partition we should connect ourselves.
    /// This partition must be a perfect match for all the processes that join the same partition,
    /// another fun extension would be to enable something like an exchange, were we could receive
    /// messages by partition prefixes.
    partition: String,
}

impl TransportConfiguration {
    /// Creates a new [`TransportConfigurationBuilder`].
    pub fn builder() -> TransportConfigurationBuilder {
        TransportConfigurationBuilder::new()
    }
}

/// A convenience builder to create the [`TransportConfiguration`] structure.
/// Here we are using a builder in order to create the [`TransportConfiguration`] structure
/// that will be used later by the [`Transport`] primitive to connect to the etcd server. We are
/// only validating that all required arguments are available, we are not doing any verification
/// on the values itself, for example, empty strings or non existent ports.
///
/// # Example
///
/// This can be used like:
///
/// ```
/// fn main() {
///     let configuration = abro::TransportConfiguration::builder()
///         .with_host("localhost")
///         .with_port(2379)
///         .with_partition("example-partition")
///         .build();
///
///     assert!(configuration.is_ok());
/// }
/// ```
///
/// # Errors
///
/// The validation is only applied when the [`TransportConfigurationBuilder::build()`] method is
/// called. It can fail if one of the required parameters are not available, the returned error
/// is an [`crate::Error`].
pub struct TransportConfigurationBuilder {
    /// Holds the `host` that will be used in [`TransportConfiguration::host`].
    host: Option<String>,

    /// Holds the `port` that will be used in [`TransportConfiguration::port`].
    port: Option<usize>,

    /// Holds the `partition` that will be used in [`TransportConfiguration::partition`].
    partition: Option<String>,
}

impl TransportConfigurationBuilder {
    fn new() -> Self {
        TransportConfigurationBuilder {
            host: None,
            port: None,
            partition: None,
        }
    }

    /// Defines the host that will be used to connect into the etcd server.
    pub fn with_host(mut self, host: &str) -> Self {
        self.host = Some(host.to_string());
        self
    }

    /// Defines the port that will be used to connect into the etcd server.
    pub fn with_port(mut self, port: usize) -> Self {
        self.port = Some(port);
        self
    }

    /// Defines the partition that the peer will be bound.
    pub fn with_partition(mut self, partition: &str) -> Self {
        self.partition = Some(partition.to_string());
        self
    }

    /// Build the [`TransportConfiguration`] structure with the given parameters.
    /// A [`crate::Result`] is returned here since we do a little validation on the parameters,
    /// verifying if we are not missing anything that is required.
    ///
    /// # Error
    ///
    /// This method will return an [`crate::Error`] if one of the required parameter is missing.
    pub fn build(self) -> crate::Result<TransportConfiguration> {
        let host = self
            .host
            .ok_or(crate::Error::MissingConfiguration(String::from(
                "Missing `host` parameter",
            )))?;
        let port = self
            .port
            .ok_or(crate::Error::MissingConfiguration(String::from(
                "Missing `port` parameter",
            )))?;
        let partition = self
            .partition
            .ok_or(crate::Error::MissingConfiguration(String::from(
                "Missing `partition` parameter",
            )))?;

        Ok(TransportConfiguration {
            address: format!("{}:{}", host, port),
            partition,
        })
    }
}

/// The transport primitive that the user will interact to send and receive messages.
pub struct Transport {
    /// Holds the wrapper around the [`etcd_client`]. The transport primitive will transform
    /// request from the user space to the format expected by [`EtcdWrapper`].
    client: EtcdWrapper,
}

/// The structure used by users to broadcast the content to the given destination.
pub struct Message {
    /// The destination identifies a partition, when we send a message to this partition all peers
    /// that are connected to it will receive the message or none will.
    destination: String,

    /// The content that we will broadcast to the given partition.
    content: String,
}

impl Message {
    /// Creates a new message, identifying the destination and the content must be serializable.
    pub fn new(destination: &str, content: impl Into<String>) -> Self {
        Message {
            destination: destination.to_string(),
            content: content.into(),
        }
    }
}

impl Transport {
    /// Create a new transport primitive.
    ///
    /// When creating the primitive we will also create the etcd client and will try to connect to
    /// the specified etcd server, so we can fail if the server is unreachable.
    ///
    /// # Example
    ///
    /// ```no_run
    /// #[tokio::main]
    /// async fn main() {
    ///     let configuration = abro::TransportConfiguration::builder()
    ///         .with_host("localhost")
    ///         .with_port(2379)
    ///         .with_partition("example-partition")
    ///         .build().unwrap();
    ///
    ///     let transport = abro::Transport::new(configuration).await;
    ///
    ///     assert!(transport.is_ok());
    /// }
    /// ```
    ///
    /// In this example we created a new [`TransportConfiguration`] using the
    /// [`TransportConfigurationBuilder`], then we use the `new` method to try to create a new
    /// [`Transport`] primitive. Note that this method is asynchronous, thus the `main` function
    /// is also `async`.
    ///
    /// # Errors
    ///
    /// This method can fail if we are not able to connect to the etcd server using the parameters
    /// present in the [`TransportConfiguration`], we will return a [`crate::Error`].
    pub async fn new(configuration: TransportConfiguration) -> crate::Result<Self> {
        let client = connect(&configuration.address, &configuration.partition).await?;
        Ok(Transport { client })
    }

    /// Broadcast the given message.
    ///
    /// We are receiving a message that identifies a partition in which we will broadcast the
    /// content. All peers that are connected to the given partition will either receive the
    /// content or none will. This can fail with an [`crate::Error`].
    ///
    /// # Example
    ///
    /// We first create a new transport primitive using the [`Transport::new`] method. The next
    /// steps to send a message are first creating the message:
    ///
    /// ```
    /// use abro::transport::Message;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let message = Message::new("destination-partition", "Hello, world!");
    /// # }
    /// ```
    ///
    /// Then we can use the [`Transport`] primitive created early to send the newly created message.
    /// Remember that under the hood we are writing a value to a distributed KV store, so we need that
    /// the [`Transport`] primitive to be mutable.
    ///
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// #     let configuration = abro::TransportConfiguration::builder()
    /// #         .with_host("localhost")
    /// #         .with_port(2379)
    /// #         .with_partition("example-partition")
    /// #         .build().unwrap();
    /// #     let mut  transport = abro::Transport::new(configuration).await.unwrap();
    /// #     let message = abro::Message::new("destination-partition", "Hello, world!");
    /// let result = transport.send(message).await;
    /// assert!(result.is_ok());
    /// # }
    /// ```
    pub async fn send(&mut self, message: Message) -> crate::Result<()> {
        self.client
            .write(&message.destination, message.content)
            .await
    }

    /// Listen for messages that were broadcast to the current partition.
    /// This is a blocking call, so its up to the user to start it the way that better fits the
    /// use case. This will receive a callback to notify any time a new message is received in
    /// the current peer.
    ///
    /// Since this method can run 'eternally', the callback function must tell the listener if
    /// it must keep running. If the callback returns `false` or return an [`crate::Error`], then
    /// we will break the loop and return, while the callback returns `true` we will keep listening
    /// for new messages.
    ///
    /// This method has some limitations, verify in the library section for the limitations and the
    /// [`EtcdWrapper::watch`] method for some comments.
    pub async fn listen<F, T>(&mut self, f: F)
    where
        F: Fn(crate::Result<String>) -> T,
        T: Future<Output = crate::Result<bool>>,
    {
        let stream = self.client.watch().await;
        pin_mut!(stream);
        while let Some(data) = stream.next().await {
            let should_continue = match f(data).await {
                Ok(v) => v,
                Err(_) => false,
            };

            if !should_continue {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::Message;
    use crate::{Transport, TransportConfiguration};

    #[ignore]
    #[tokio::test]
    async fn should_listen_for_messages() {
        let mut st_transport = create_transport("partition-1").await;
        let mut nd_transport = create_transport("partition-2").await;

        async fn message_handler(data: crate::Result<String>) -> crate::Result<bool> {
            assert!(data.is_ok());
            Ok(false)
        }
        let handle = tokio::spawn(async move { st_transport.listen(message_handler).await });

        let message = Message::new("partition-1", "hello!");
        let response = nd_transport.send(message).await;
        assert!(response.is_ok());

        let result = handle.await;
        assert!(result.is_ok());
    }

    async fn create_transport(partition: &str) -> Transport {
        let configuration = TransportConfiguration {
            address: String::from("localhost:2379"),
            partition: partition.to_string(),
        };

        let transport = Transport::new(configuration).await;

        assert!(transport.is_ok());
        transport.unwrap()
    }
}
