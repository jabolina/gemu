use std::collections::{HashMap, HashSet};

use uuid::Uuid;

/// A ballot struct that identify a single vote.
///
/// This structure is used to identify the timestamp vote from a specific partition.
struct Ballot {
    /// Partition which voted for the current timestamp.
    owner: String,

    /// The timestamp voted for.
    vote: u64,
}

/// A ballot box structure used to elect a timestamp.
///
/// This structure holds the votes from all partitions for each messages. Here we have different
/// terms for structure already presented, they are renamed in this context so it can better relate
/// with the concept for voting. A relation is presented:
///
/// * Message timestamp the same as a `vote`;
/// * Partition the same as a `voter`;
/// * Message unique identifier, is identified by `election`.
///
/// So, we can read as, a voter is casting a vote for a specific election. This can also be read as
/// a partition is proposing a timestamp for a specific message.
#[derive(Default)]
pub(crate) struct BallotBox {
    /// Hold the votes.
    ///
    /// This is mapping a message identifier represented by an [`Uuid`] to an array of votes.
    votes: HashMap<Uuid, Vec<Ballot>>,
}

impl BallotBox {
    /// Creates a new [`BallotBox`].
    pub(crate) fn new() -> Self {
        Default::default()
    }

    /// Register a new vote for a specific election.
    pub(crate) fn vote(&mut self, election: Uuid, from: &str, value: u64) {
        let ballot = Ballot {
            owner: String::from(from),
            vote: value,
        };
        self.votes
            .entry(election)
            .or_insert(Vec::new())
            .push(ballot);
    }

    /// Read all votes from a specific election.
    pub(crate) fn read_votes(&self, election: Uuid) -> Vec<u64> {
        match self.votes.get(&election) {
            Some(e) => e.iter().map(|b| b.vote).collect(),
            None => Vec::new(),
        }
    }

    /// Returns the number of unique voters for a specific election.
    pub(crate) fn unique_voters_size(&self, election: Uuid) -> usize {
        match self.votes.get(&election) {
            Some(e) => e
                .iter()
                .map(|b| b.owner.clone())
                .collect::<HashSet<String>>()
                .len(),
            None => 0,
        }
    }

    /// Finish the election removing the entry and related values.
    pub(crate) fn election_done(&mut self, election: Uuid) {
        self.votes.remove(&election);
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::internal::ballot_box::BallotBox;

    #[test]
    fn should_register_votes() {
        let mut ballot_box = BallotBox::new();
        let uuid = Uuid::new_v4();

        for i in 0..30 {
            ballot_box.vote(uuid, &format!("voter-{}", i), i);
        }

        assert_eq!(ballot_box.unique_voters_size(uuid), 30);

        let votes = ballot_box.read_votes(uuid);
        assert_eq!(votes, (0..30).collect::<Vec<u64>>());

        ballot_box.election_done(uuid);
        assert_eq!(ballot_box.unique_voters_size(uuid), 0);
    }

    #[test]
    fn should_deduplicate_voters() {
        let mut ballot_box = BallotBox::new();
        let uuid = Uuid::new_v4();

        for i in 0..30 {
            ballot_box.vote(uuid, &format!("voter-{}", i), i);
            ballot_box.vote(uuid, &format!("voter-{}", i), i);
        }

        assert_eq!(ballot_box.unique_voters_size(uuid), 30);
        assert_eq!(ballot_box.read_votes(uuid).len(), 60);
    }
}
