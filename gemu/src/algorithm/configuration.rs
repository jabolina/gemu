use crate::algorithm::AlgorithmError::ConfigurationMissing;
use crate::algorithm::{ConflictRelationship, Oracle};
use abro::{Error, TransportConfiguration};

pub struct GenericMulticastConfiguration<V> {
    pub(crate) partition: String,
    pub(crate) local_port: usize,
    pub(crate) group_configuration: abro::TransportConfiguration,
    pub(crate) conflict: Box<dyn ConflictRelationship<V> + 'static>,
    pub(crate) oracle: Box<dyn Oracle + 'static>,
}

pub struct ConfigurationBuilder<V> {
    partition: Option<String>,
    local_port: Option<usize>,
    group_port: Option<usize>,
    group_host: Option<String>,
    conflict: Option<Box<dyn ConflictRelationship<V> + 'static>>,
    oracle: Option<Box<dyn Oracle + 'static>>,
}

impl<V> GenericMulticastConfiguration<V>
where
    V: From<String>,
{
    pub fn builder() -> ConfigurationBuilder<V> {
        ConfigurationBuilder::default()
    }
}

impl<V> ConfigurationBuilder<V>
where
    V: From<String>,
{
    pub fn with_partition(mut self, partition: &str) -> Self {
        self.partition = Some(String::from(partition));
        self
    }

    pub fn with_port(mut self, port: usize) -> Self {
        self.local_port = Some(port);
        self
    }

    pub fn with_group_port(mut self, port: usize) -> Self {
        self.group_port = Some(port);
        self
    }

    pub fn with_group_host(mut self, host: &str) -> Self {
        self.group_host = Some(String::from(host));
        self
    }

    pub fn with_conflict(mut self, conflict: impl ConflictRelationship<V> + 'static) -> Self {
        self.conflict = Some(Box::new(conflict));
        self
    }

    pub fn with_oracle(mut self, oracle: impl Oracle + 'static) -> Self {
        self.oracle = Some(Box::new(oracle));
        self
    }

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
