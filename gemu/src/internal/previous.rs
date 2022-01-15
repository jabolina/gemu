//! The `previous` structure from the specification.
//!
//! This structure is used to hold messages so is possible to verify for possible conflicts, with
//! only a simple API exposed internally during the algorithm execution. This structure should hold
//! messages without duplicating entries using this structure to know for possible conflicts, so it
//! needs to know the [`ConflictRelationship`].
//!
//! The [`ConflictRelationship`] is generic over a value, so we need to know how to parse the
//! [`Message`] string content back to the relationship generic value. We could in later times
//! create a facade for serialization and deserialization and try to do something more fancy.
//!
//! [`ConflictRelationship`]: crate::algorithm::ConflictRelationship
//! [`Message`]: crate::algorithm::Message
use std::collections::HashSet;

use crate::algorithm::ConflictRelationship;
use crate::algorithm::Message;

/// The `Previous` structure.
///
/// This structure keep information about processing message and the conflict relationship. This
/// is used to verify for conflicts and apply the generic behavior of the algorithm.
struct PreviousMessages<'a, V> {
    // The conflict relationship.
    relationship: Box<dyn ConflictRelationship<V> + 'a>,

    // The set of messages.
    messages: HashSet<Message>,
}

impl<'a, V> PreviousMessages<'a, V>
where
    V: From<String>,
{
    /// Creates a new [`Previous`] structure using the given [`ConflictRelationship`].
    pub(crate) fn new(relationship: impl ConflictRelationship<V> + 'a) -> Self {
        return PreviousMessages {
            relationship: Box::new(relationship),
            messages: HashSet::new(),
        };
    }

    /// Insert a new message into the structure.
    pub(crate) fn append(&mut self, value: Message) {
        self.messages.insert(value);
    }

    /// Clear the structure.
    pub(crate) fn clear(&mut self) {
        self.messages.clear();
    }

    /// Verify if the given message conflict with other messages.
    ///
    /// This method represents the set operation to verify if exists a message `n` in the
    /// `Previous` set that conflicts with the message `m`.
    ///
    /// ```text
    /// ∃ n ∈ Previous: n ~ m
    /// ```
    pub(crate) fn conflicts(&self, value: Message) -> bool {
        let rhs = V::from(String::from(value.content()));
        for lhs_message in self.messages.iter() {
            // If serialization is expensive this could be hard to do.
            let lhs = V::from(String::from(lhs_message.content()));
            if self.relationship.conflict(&lhs, &rhs) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithm::{ConflictRelationship, Message};
    use crate::internal::previous::PreviousMessages;
    use uuid::Uuid;

    struct StringConflict;
    impl ConflictRelationship<String> for StringConflict {
        fn conflict(&self, lhs: &String, rhs: &String) -> bool {
            String::from(lhs) == String::from(rhs)
        }
    }

    #[test]
    fn should_apply_operations() {
        let mut previous = PreviousMessages::new(StringConflict);
        let message = Message::build("value", Vec::new());

        previous.append(message);

        assert!(previous.conflicts(Message::build("value", Vec::new())));
        assert!(!previous.conflicts(Message::build("other-value", Vec::new())));

        previous.clear();

        assert!(!previous.conflicts(Message::build("value", Vec::new())));
    }

    #[test]
    fn should_not_duplicate() {
        let mut previous = PreviousMessages::new(StringConflict);
        let id = Uuid::new_v4();
        let message = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S0;dest1,dest2;Broadcast", id);

        for _ in 0..15 {
            let message = Message::from(message.clone());
            previous.append(message);
        }

        assert_eq!(previous.messages.len(), 1);
    }
}
