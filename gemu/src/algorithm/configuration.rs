use crate::algorithm::AlgorithmError::ConfigurationMissing;
use crate::algorithm::{ConflictRelationship, Oracle};
use abro::{Error, TransportConfiguration};

/// This structure holds the configuration that will be used for the algorithm.
///
/// This is generic over the type the conflict relationship will work with.
pub struct GenericMulticastConfiguration<V> {
    // Identify the partition this peer belongs to, this is used also for subscribing to receive
    // incoming broadcast message. This is also used to broadcast the message internally.
    pub(crate) partition: String,

    // The port which the peer will bind locally for the simple process communication.
    pub(crate) local_port: usize,

    // The group configuration that will be used to connect to the server. The partition value is
    // used in here as well.
    pub(crate) group_configuration: abro::TransportConfiguration,

    // The conflict relationship which is generic over some value. This structure must be static
    // because it must life throughout the complete algorithm lifetime.
    pub(crate) conflict: Box<dyn ConflictRelationship<V> + 'static>,

    // The oracle structure. This will be used when sending message with the process communication,
    // this way we can still send messages to partitions but convert a partition back to addresses.
    pub(crate) oracle: Box<dyn Oracle + 'static>,
}

/// Convenience structure to help building a [`GenericMulticastConfiguration`] structure.
pub struct ConfigurationBuilder<V> {
    // Holds the partition the peer belongs to. This is a required property.
    partition: Option<String>,

    // The port which the peer will bind locally. This is an optional property, defaults to 12233.
    local_port: Option<usize>,

    // The group server partition, this is an optional property, default to etcd port at 2379.
    group_port: Option<usize>,

    // The group host server, this is an optional property, default to localhost.
    group_host: Option<String>,

    // The conflict relationship to verify for conflicting message. This is a required property.
    conflict: Option<Box<dyn ConflictRelationship<V> + 'static>>,

    // The oracle structure to identify the processes within a partition. This is required.
    oracle: Option<Box<dyn Oracle + 'static>>,
}

impl<V> GenericMulticastConfiguration<V>
where
    V: From<String>,
{
    /// Create the convenience builder for the configuration.
    ///
    /// The data which is handled by the conflict relationship must be convertable from string.
    pub fn builder() -> ConfigurationBuilder<V> {
        ConfigurationBuilder::default()
    }
}

impl<V> ConfigurationBuilder<V>
where
    V: From<String>,
{
    /// Sets the partition to be used.
    pub fn with_partition(mut self, partition: &str) -> Self {
        self.partition = Some(String::from(partition));
        self
    }

    /// Sets the local process port.
    pub fn with_port(mut self, port: usize) -> Self {
        self.local_port = Some(port);
        self
    }

    /// Sets the group server port.
    pub fn with_group_port(mut self, port: usize) -> Self {
        self.group_port = Some(port);
        self
    }

    /// Sets the group server host.
    pub fn with_group_host(mut self, host: &str) -> Self {
        self.group_host = Some(String::from(host));
        self
    }

    /// Sets the conflict relationship.
    ///
    /// The relationship must be static since it will be used during the complete application life.
    pub fn with_conflict(mut self, conflict: impl ConflictRelationship<V> + 'static) -> Self {
        self.conflict = Some(Box::new(conflict));
        self
    }

    /// Sets the oracle.
    ///
    /// The oracle must be static since it will be used during the complete application life.
    pub fn with_oracle(mut self, oracle: impl Oracle + 'static) -> Self {
        self.oracle = Some(Box::new(oracle));
        self
    }

    /// Build the [`GenericMulticastConfiguration`].
    ///
    /// This will create the configuration with the given values. If not provided, some of the
    /// properties will use a default value.
    ///
    /// # Errors
    ///
    /// This will return an error if a required property was not provided.
    pub fn build(self) -> crate::Result<GenericMulticastConfiguration<V>> {
        let partition = self
            .partition
            .ok_or(ConfigurationMissing(String::from("Missing partition name")))?;
        let local_port = self.local_port.unwrap_or(12233);
        let group_port = self.group_port.unwrap_or(2379);
        let group_host = self.group_host.unwrap_or(String::from("localhost"));

        let group_configuration = TransportConfiguration::builder()
            .with_partition(&partition)
            .with_port(group_port)
            .with_host(&group_host)
            .build()?;
        let conflict = self.conflict.ok_or(ConfigurationMissing(String::from(
            "Missing conflict relationship",
        )))?;
        let oracle = self
            .oracle
            .ok_or(ConfigurationMissing(String::from("Missing oracle")))?;

        Ok(GenericMulticastConfiguration {
            partition,
            local_port,
            group_configuration,
            conflict,
            oracle,
        })
    }
}

/// Implements a default value for the [`ConfigurationBuilder`].
impl<V> Default for ConfigurationBuilder<V>
where
    V: From<String>,
{
    fn default() -> Self {
        ConfigurationBuilder {
            partition: None,
            local_port: None,
            group_port: None,
            group_host: None,
            conflict: None,
            oracle: None,
        }
    }
}

impl From<abro::Error> for crate::Error {
    fn from(_: Error) -> Self {
        todo!()
    }
}
