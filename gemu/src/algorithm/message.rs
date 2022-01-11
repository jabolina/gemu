//! Here is implemented everything about the protocol message itself.
//!
//! Here can be found about the [`Message`] object, which is the actual object transport within the
//! protocol, the [`MessageType`] used to identify which type of message was received and the
//! [`MessageStatus`] which identify the processing status of the current message.
//!
//! # [`MessageType`]
//!
//! This enumeration is used so is possible to distinguish the received message, so is not needed
//! to poll from different channels we are adding more data to be transported. This could change in
//! the future to something else, but for now this is enough.
//!
//! # [`MessageStatus`]
//!
//! Identify the current status of the message. This values follows the protocol definition, so we
//! have that:
//!
//! * [`MessageStatus::S0`]: The first state, a message without an associated timestamp.
//! * [`MessageStatus::S1`]: A message have the group timestamp, which is not the final value yet.
//! * [`MessageStatus::S2`]: A message have the final timestamp, the group be synced.
//! * [`MessageStatus::S3`]: A message have the final timestamp and is ready to be delivered.
//!
//! # [`Message`]
//!
//! The message structure itself, this complete payload will be transport between peers. The struct
//! here contains all the required properties from the protocol with some additional properties for
//! convenience. The additional properties are the [`r#type`] which is a [`MessageType`] and the
//! [`content`] field, which is used to transport actual user data. The remaining properties are
//! defined by the protocol:
//!
//! * `id`: A global unique identifier, this value is static and should not change. We should be
//!     able to compare this value for ordering between messages. This is generated internally.
//! * `timestamp`: An unsigned int with 128 bits. This contains the message timestamp, which will
//!     be defined during the algorithm execution. The initial value is 0.
//! * `status`: Identify the message current status, is the [`MessageStatus`] enumeration.
//! * `destination`: A vector identifying the final destination of the message. This value does not
//!     change.
//!
//! ## Serialization / Deserialization
//!
//! To send the message between peers, the structure must be serializable. Since we do not want to
//! introduce a library here to handle only a single specific structure, we implemented the struct
//! serialization by hand. In this approach we only append the data separated by `;`, the user
//! content which is transported is encoded in base 64. The approach is only to append each of the
//! fields into a String and to deserialize, to just split the values and create the structure
//! back again.
//!
//! Although the code is not nice, I think this is better than to introduce a serialization library
//! here to handle just this structure. This has that big problem of any change introduced in the
//! [`Message`] structure will lead to changes in the serialization / deserialization process.
//!
//! In the deserialization process we are only splitting the String by the defined separator and
//! using the direct expected index. This is sooo error prone by eventual changes applied to the
//! structure itself, index must be adjusted to access to correct location in the serialization.
//!
//! ## Comparison and ordering
//!
//! This maybe is not clear by the protocol specification? Must the message structure must have a
//! specific way to be compared and ordered. Here we also implement the expected behavior for this
//! comparisons.
//!
//! A message will be considered equal to another message directly by the `id` property. If two
//! identifiers are considered equals, the other properties are not compared. This comparison is
//! also used to identify for messages that are held by protocol structures.
//!
//! The ordering is also implemented here, which is pretty simple. Only the `timestamp` and the
//! `id` are used for ordering. Messages are ordered by the timestamps, if both messages have the
//! same timestamps then the unique identifier is used to break ties. So is not "allowed" to have
//! a `<=` or `>=` operators, since we always will breaks ties.
use crate::algorithm::message::MessageStatus::S0;
use crate::algorithm::message::MessageType::Broadcast;
use std::cmp::Ordering;
use std::str::FromStr;

/// An internal enumeration to identify the type of the received message. This is used so we can
/// easily identify the primitive which was used to send the message.
#[derive(Debug)]
pub(crate) enum MessageType {
    /// The message was received through a broadcast primitive.
    Broadcast,

    /// The message was received through a process primitive.
    Process,
}

/// Identify the message current status.
///
/// This follows the same name from the protocol specification and also means the same thing.
#[derive(Debug)]
pub(crate) enum MessageStatus {
    /// Message without any timestamp associated yet.
    S0,

    /// Message with the group timestamp.
    S1,

    /// Message with a final timestamp, but the group need to be synchronized.
    S2,

    /// Message with a final timestamp and ready to be delivered.
    S3,
}

/// The concrete protocol message.
///
/// This is the structure that is exchanged between processes. This contains the properties used by
/// the protocol with some additional properties.
#[derive(Debug)]
pub(crate) struct Message {
    id: uuid::Uuid,
    content: String,
    timestamp: u128,
    status: MessageStatus,
    destination: Vec<String>,
    r#type: MessageType,
}

impl Message {
    /// Build a new message.
    ///
    /// This is used to create the initial message at the start of the algorithm. Will be used the
    /// given content and destination, all the other properties will be the default initial values.
    /// The initial values are status `S0`, timestamp 0, a broadcast type and the newly generated
    /// unique identifier.
    ///
    /// This should be used only at the start of the protocol, when issuing a new message.
    pub(crate) fn build(content: &str, destination: Vec<&str>) -> Message {
        let destination = destination.iter().map(|&s| String::from(s)).collect();
        let id = uuid::Uuid::new_v4();

        Message {
            id,
            destination,
            timestamp: 0,
            content: String::from(content),
            status: S0,
            r#type: Broadcast,
        }
    }

    #[inline]
    pub(crate) fn r#type(&self) -> &MessageType {
        &self.r#type
    }

    #[inline]
    pub(crate) fn destination(&self) -> &Vec<String> {
        &self.destination
    }

    /// Update the message timestamp with the given value.
    ///
    /// We will only change the timestamp if the message current status is either `S0` or `S1`.
    /// Since any other status means that the final timestamp is already selected. This should be
    /// used before updating the message status.
    pub(crate) fn update_timestamp(mut self, timestamp: u128) -> Self {
        match self.status {
            MessageStatus::S0 | MessageStatus::S1 => self.timestamp = timestamp,
            _ => {}
        }
        self
    }

    /// Update the message status with the given value.
    pub(crate) fn update_status(mut self, status: MessageStatus) -> Self {
        self.status = status;
        self
    }

    /// Update the message type with the given value.
    pub(crate) fn update_type(mut self, r#type: MessageType) -> Self {
        self.r#type = r#type;
        self
    }
}

/// Serialize the [`MessageStatus`]
impl Into<String> for MessageStatus {
    fn into(self) -> String {
        let value = match self {
            MessageStatus::S0 => "S0",
            MessageStatus::S1 => "S1",
            MessageStatus::S2 => "S2",
            MessageStatus::S3 => "S3",
        };
        String::from(value)
    }
}

/// Deserialize the [`MessageStatus`]
impl From<&str> for MessageStatus {
    fn from(s: &str) -> Self {
        match s {
            "S0" => MessageStatus::S0,
            "S1" => MessageStatus::S1,
            "S2" => MessageStatus::S2,
            "S3" => MessageStatus::S3,
            _ => panic!("{} is not MessageStatus", s),
        }
    }
}

/// Serialize the [`MessageType`]
impl Into<String> for MessageType {
    fn into(self) -> String {
        let value = match self {
            MessageType::Process => "Process",
            MessageType::Broadcast => "Broadcast",
        };
        String::from(value)
    }
}

/// Deserialize the [`MessageType`]
impl From<&str> for MessageType {
    fn from(s: &str) -> Self {
        match s {
            "Process" => MessageType::Process,
            "Broadcast" => MessageType::Broadcast,
            _ => panic!("{} is not a MessageType", s),
        }
    }
}

/// Serialize the [`Message`] structure.
///
/// We will append all values to a single string using a `;` separator. The values must have an
/// expected location in order to deserialize correctly. Before joining everything into a single
/// String, first we base 64 encode the user content. Then all values are transformed into a String
/// with the indexes:
///
/// 0. The message identifier.
/// 1. The base 64 encoded user content.
/// 2. The message timestamp.
/// 3. The message status.
/// 4. The message destination. We join all destination into a single String as well.
/// 5. The message type.
///
/// This indexes are established so deserialization will access them directly.
///
/// # Errors
///
/// Although this serialization will not return any error, some of the arguments could lead the
/// deserialization to panic. The only value which is encoded is the user argument, if for some
/// reason the destination contains the separator, the deserialization will panic. This could be
/// solved by also encoding the destination values.
impl Into<String> for Message {
    fn into(self) -> String {
        let status: String = MessageStatus::into(self.status);
        let message_type: String = MessageType::into(self.r#type);
        vec![
            self.id.to_string(),
            base64::encode(self.content),
            self.timestamp.to_string(),
            status,
            self.destination.join(","),
            message_type,
        ]
        .join(";")
    }
}

/// Deserialize the String back to the [`Message`] structure.
///
/// We first split the string by the separator and then start to access each index defined
/// previously during serialization. During this process if anything unexpected happen we will
/// panic right away.
impl From<String> for Message {
    fn from(data: String) -> Self {
        let splitted: Vec<&str> = data.split(";").collect();

        // The vector should have always 6 elements, which is the values defined previously. If
        // anything is different, we will only panic and will not try anything fancy.
        assert_eq!(splitted.len(), 6, "Malformed string");

        let decoded = base64::decode(splitted[1]).expect("Should decode content");
        let content = String::from_utf8(decoded).expect("Should parse to string");
        let destination = splitted[4].split(",").map(|s| String::from(s)).collect();

        Message {
            id: uuid::Uuid::from_str(splitted[0]).expect("Should have identifier"),
            content,
            timestamp: splitted[2].parse().expect("Should have timestamp"),
            status: MessageStatus::from(splitted[3]),
            destination,
            r#type: MessageType::from(splitted[5]),
        }
    }
}

impl PartialEq for Message {
    /// Verify if both messages are equals.
    ///
    /// We verify only the message identifier to compare if the messages are equal. We do not check
    /// any other property.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for Message {
    /// Implement partial order for the [`Message`] structure.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }

        if self < other {
            return Some(Ordering::Less);
        }

        Some(Ordering::Greater)
    }

    /// Verify if a message is less than other message.
    ///
    /// Here we use the strategy defined by the protocol itself. We use the timestamps for ordering
    /// both messages, if the timestamps are equal then we use the message unique identifier.
    fn lt(&self, other: &Self) -> bool {
        if self.timestamp == other.timestamp {
            return self.id < other.id;
        }
        self.timestamp < other.timestamp
    }

    /// Messages can only be small or greater, should not be possible to use `<=`.
    fn le(&self, _: &Self) -> bool {
        panic!("Operator not allowed")
    }

    /// Verify if the message is greater than some other message.
    fn gt(&self, other: &Self) -> bool {
        !self.lt(other)
    }

    /// Messages can only be small or greater, should not be possible to use `>=`.
    fn ge(&self, _: &Self) -> bool {
        panic!("Operator not allowed")
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithm::message::{Message, MessageStatus, MessageType};
    use std::str::FromStr;

    #[test]
    fn should_serialize_and_deserialize() {
        let message = Message::build("hello, world!", vec!["dest1", "dest2"]);
        let id = message.id.to_string();
        let expected_serialized = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S0;dest1,dest2;Broadcast", id);
        let expected_deserialized = Message {
            id: uuid::Uuid::from_str(&id).unwrap(),
            content: String::from("hello, world!"),
            timestamp: 0,
            status: MessageStatus::S0,
            destination: vec![String::from("dest1"), String::from("dest2")],
            r#type: MessageType::Broadcast,
        };

        let serialized: String = Message::into(message);
        assert_eq!(serialized, expected_serialized);

        let deserialized = Message::from(serialized);
        assert_eq!(deserialized, expected_deserialized)
    }

    #[test]
    fn should_return_comparison_correctly() {
        let greater = Message::build("hello, world!", vec!["dest1", "dest2"]).update_timestamp(2);
        let smaller = Message::build("hello, world!", vec!["dest1", "dest2"]).update_timestamp(1);

        assert!(smaller < greater);
        assert!(greater > smaller);
        assert_ne!(smaller, greater);
        assert_eq!(smaller, smaller);
        assert_eq!(greater, greater);
    }
}
