use crate::algorithm::message::MessageStatus::S0;
use crate::algorithm::message::MessageType::Broadcast;

// todo: we must transport the content along with protocol metadata.
// the underlying implementation uses only destination and content, the content must have all meta.
// We also must implement a serialization and deserialization for all this data.

pub(crate) enum MessageType {
    Broadcast,
    Process,
}

pub(crate) enum MessageStatus {
    S0,
    S1,
    S2,
    S3,
}

pub(crate) struct Message {
    content: String,
    timestamp: u128,
    origin: String,
    status: MessageStatus,
    destination: Vec<String>,
    r#type: MessageType,
}

impl Message {
    pub(crate) fn parse(content: String) -> Self {
        Message {
            content,
            timestamp: 0,
            origin: String::new(),
            status: S0,
            destination: vec![],
            r#type: Broadcast,
        }
    }

    #[inline]
    pub(crate) fn r#type(&self) -> &MessageType {
        &self.r#type
    }

    #[inline]
    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    #[inline]
    pub(crate) fn destination(&self) -> &Vec<String> {
        &self.destination
    }
}
