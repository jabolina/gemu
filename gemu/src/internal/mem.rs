use std::borrow::Borrow;
use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;

/// The [`PriorityQueue`] structure, this simulates the algorithm `mem` structure.
///
/// This structure hold some type of data, the data must implement the [`Eq`] and [`PartialOrd`]
/// traits, the elements will be kept into a priority queue, where the smallest element will be at
/// the queue head. This structure also behaves like an ordered set, keeping the smallest element
/// at the head and do not duplicate elements in the list.
///
/// Every time a change is applied to the head, the `verification` method will be used to identify
/// if the element should be notified through the `ready` channel. Sending the element through the
/// channel means that it was removed from the queue.
///
/// Note that this is a super simple implementation for a priority queue, there is nothing fancy
/// here, we are really using a whatever big O algorithm for this queue. Note that also that this
/// is created to be used in a single threaded and currently is not capable of handling concurrent
/// operations.
struct PriorityQueue<T> {
    // The actual values stored.
    values: Vec<T>,

    // Channel used to notify about head changes.
    ready: mpsc::Sender<T>,

    // Verify if a notification should be sent through the channel.
    verification: fn(&T) -> bool,
}

impl<T> PriorityQueue<T>
where
    T: Eq + PartialOrd,
{
    /// Creates a new [`PriorityQueue`].
    ///
    /// This queue will use the given channel to notify about changes in the head of the queue.
    /// Using the verification closure received to verify if the change should be notified using
    /// the channel.
    ///
    /// The closure should be a lightweight verification, since it is used frequently.
    fn new(channel: mpsc::Sender<T>, verification: fn(&T) -> bool) -> Self {
        PriorityQueue {
            values: Vec::new(),
            ready: channel,
            verification,
        }
    }

    /// Insert a new element into the priority queue.
    ///
    /// If already exists an entry for the given value, it will be update, otherwise, a new entry
    /// will be added to the queue. Then the elements will be verified again for possible changes.
    async fn append(&mut self, value: T) {
        let mut previous = self.values.len();
        match self.values.iter().position(|v| v.eq(&value)) {
            Some(index) => {
                self.values[index] = value;

                // If the entry is update, this must happen so the rearranging does not fail.
                previous -= 1;

                // This means we directly updated the head of the queue, so we first verify if the
                // notification must be send before proceeding to reorganize the structure.
                if index == 0 {
                    self.head_changed().await;
                }
            }
            None => self.values.push(value),
        }

        // Is possible that an update to the head could completely drain the queue.
        if !self.values.is_empty() {
            self.bubble_up(0, previous).await;
        }
    }

    /// Removes the given element from the queue.
    ///
    /// This will remove the given element from the queue, and using the verification to know if it
    /// should send a notification through the channel. Even without sending a notification it will
    /// remove the element from the queue.
    async fn remove(&mut self, value: T) {
        match self.values.iter().position(|e| e.eq(&value)) {
            Some(index) => {
                let removed = self.values.swap_remove(index);

                if (self.verification)(&removed) {
                    let _ = self.ready.send(removed).await;
                }

                if !self.values.is_empty() {
                    self.sift_down(0).await;
                }
            }
            None => {}
        }
    }

    /// Execute a closure against the queue values.
    ///
    /// This will stop at the first `true` returned by the closure.
    fn contains(&self, comparator: fn(&T) -> bool) -> bool {
        for v in self.values.iter() {
            if comparator(v) {
                return true;
            }
        }
        false
    }

    /// Reorganize the queue after a new element is added.
    async fn bubble_up(&mut self, start: usize, mut pos: usize) {
        let mut head_changed = false;

        while pos > start {
            let parent = (pos - 1) / 2;

            if self.values[pos] < self.values[parent] {
                // If an element is exchanged with the element at 0 means that the head changed.
                // If this happen only once, we will keep the flag active.
                head_changed = head_changed || parent == 0;
                self.values.swap(pos, parent);
                pos = parent;
            } else {
                break;
            }
        }

        // If the head changed we need to verify if a notification must be sent.
        if head_changed {
            self.head_changed().await;
        }
    }

    /// Reorganize the queue after an element is removed.
    ///
    /// This function is not a simple `async` method because it is used recursively, so it need to
    /// box the returned future, then is pinned so we can `await` on it.
    fn sift_down(&mut self, mut pos: usize) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            let end = self.values.len() - 1;
            let mut child = 2 * pos + 1;

            while child <= end {
                let right = child + 1;

                if right <= end && self.values[child] > self.values[right] {
                    child = right;
                }

                if self.values[pos] > self.values[child] {
                    self.values.swap(pos, child);
                    pos = child;
                    child = 2 * pos + 1;
                } else {
                    break;
                }
            }

            // Since we execute this after removing an element, we are completely sure that the
            // head has changed.
            self.head_changed().await;
        })
    }

    /// Use to verify the head after a change.
    ///
    /// This will cause the queue to change, if the head is notified it will be removed from the
    /// queue. This can lead to the queue being totally drained with chained changes applied.
    async fn head_changed(&mut self) {
        match self.values.len() {
            0 => {}
            _ => {
                let head = self.values[0].borrow();
                if (self.verification)(head) {
                    let head = self.values.swap_remove(0);
                    let _ = self.ready.send(head).await;

                    if !self.values.is_empty() {
                        self.sift_down(0).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::algorithm::Message;
    use crate::algorithm::MessageStatus::{S0, S1, S3};
    use crate::internal::mem::PriorityQueue;

    #[tokio::test]
    async fn should_not_duplicate() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let mut mem = PriorityQueue::new(tx, |_| false);
        let id = Uuid::new_v4();
        let message = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S0;dest1,dest2;Broadcast", id);

        mem.append(Message::from(message.clone())).await;
        mem.append(Message::from(message.clone())).await;

        assert_eq!(mem.values.len(), 1);
    }

    #[tokio::test]
    async fn should_insert_and_be_notified() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let mut mem = PriorityQueue::new(tx, |v: &Message| *(v.status()) == S3);

        for i in 0..10 {
            let id = Uuid::new_v4();
            let message = format!(
                "{};aGVsbG8sIHdvcmxkIQ==;{};S0;dest1,dest2;Broadcast",
                id,
                i + 1
            );
            mem.append(Message::from(message)).await;
        }

        let id = Uuid::new_v4();
        let message = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S3;dest1,dest2;Broadcast", id);
        mem.append(Message::from(message.clone())).await;

        let delivered = tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let delivered = rx.recv().await;
            assert!(delivered.is_some());
            delivered.unwrap()
        })
        .await;

        assert!(delivered.is_ok());
        let delivered = delivered.unwrap();
        assert_eq!(delivered, Message::from(message));
        assert_eq!(mem.values.len(), 10);
    }

    #[tokio::test]
    async fn should_insert_and_verify_contains() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let mut mem = PriorityQueue::new(tx, |_| false);

        for _ in 0..10 {
            let id = Uuid::new_v4();
            let message = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S0;dest1,dest2;Broadcast", id);
            mem.append(Message::from(message)).await;
        }

        assert!(mem.contains(|v| *(v.status()) == S0));
        assert!(!mem.contains(|v| *(v.status()) == S1));
    }

    #[tokio::test]
    async fn should_insert_and_remove() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(11);
        let mut mem = PriorityQueue::new(tx, |v: &Message| *(v.status()) == S3);
        let mut ids = Vec::new();

        for i in (0..5).rev() {
            let id = Uuid::new_v4();
            ids.push((id.clone(), i));
            let message = format!("{};b2k=;{};S0;dest1,dest2;Broadcast", id, i);
            mem.append(Message::from(message)).await;
        }

        assert_eq!(mem.values.len(), 5);

        for (id, i) in ids.iter() {
            let message = format!("{};b2k=;{};S3;dest1,dest2;Broadcast", id, i);
            mem.append(Message::from(message)).await;
        }

        for i in 0..5 {
            let received = rx.recv().await;
            assert!(received.is_some());
            let received = received.unwrap();

            assert_eq!(received.timestamp(), i);
        }

        assert_eq!(mem.values.len(), 0);
    }

    #[tokio::test]
    async fn should_insert_and_manually_remove() {
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let mut mem = PriorityQueue::new(tx, |_| false);
        let id = Uuid::new_v4();
        let message = format!("{};aGVsbG8sIHdvcmxkIQ==;0;S0;dest1,dest2;Broadcast", id);

        mem.append(Message::from(message.clone())).await;
        assert_eq!(mem.values.len(), 1);

        mem.remove(Message::from(message.clone())).await;
        assert_eq!(mem.values.len(), 0);
    }
}
