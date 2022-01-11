use crate::algorithm::handler::NextStep::NoOp;
use crate::algorithm::message::{Message, MessageType};
use crate::algorithm::AlgorithmResult;
use tokio::sync::{broadcast, mpsc};

/// The Generic Multicast implementation itself.
///
/// This structure holds enough information that will be used during the algorithm execution and is
/// responsible to implement the specification. This structure will listen for incoming messages
/// from the underlying transport primitives, and for each received message start processing
/// accordingly with the algorithm specification.
pub(crate) struct GenericMulticastReceiver {
    // Receive message that should be handled by the algorithm.
    consumer: mpsc::Receiver<String>,

    // Send messages using the underlying communication primitives.
    publisher: mpsc::Sender<Message>,
}

/// This enumeration is just an implementation design.
///
/// After executing a step from the specification, instead of executing something inside the step
/// itself, for example, broadcasting the message, we use this to detach this process from the
/// step implementation.
enum NextStep {
    // No operation required.
    NoOp,

    // The message must be sent to all processes.
    SendProcesses(Message),

    // The message must be broadcast to the current group.
    SendGroup(Message),
}

impl GenericMulticastReceiver {
    /// Creates the [`GenericMulticastReceiver`].
    ///
    /// The structure will receive incoming messages from the `consumer` channel and when needed,
    /// messages will be sent through the `publisher` channel.
    pub(crate) fn new(consumer: mpsc::Receiver<String>, publisher: mpsc::Sender<Message>) -> Self {
        GenericMulticastReceiver {
            consumer,
            publisher,
        }
    }

    /// Poll for incoming messages.
    ///
    /// The entrypoint for messages to be processed by the algorithm. This method must be called so
    /// we can start processing messages and actually using the algorithm. The structure will only
    /// poll for incoming messages and process messages using the generic multicast message.
    pub(crate) async fn poll(&mut self, mut shutdown_rx: broadcast::Receiver<()>) {
        loop {
            tokio::select! {
                data = self.consumer.recv() => {
                    if let Some(data) = data {
                        self.process(data).await;
                    }
                }
                _ = shutdown_rx.recv() => break
            }
        }
    }

    /// Process the received data.
    ///
    /// The received data is handled by the algorithm implementation. The first step is to parse
    /// the String to the [`Message`] format. Then send the message to be executed in the correct
    /// step and then execute the next step requirement, for example, broadcasting the message to
    /// the current group.
    ///
    /// # Errors
    ///
    /// This method will panic if the receive data is not a [`Message`], since the parsing method
    /// will panic for unexpected data. We could change this in the future, to avoid any problem.
    async fn process(&mut self, data: String) {
        let message = Message::from(data);
        let next = match message.r#type() {
            // Messages received through broadcast are either in the beginning or synchronizing the
            // group clock.
            MessageType::Broadcast => self.compute_group_timestamp(message).await,

            // Messages received through the process primitive means we are exchanging timestamp
            // between the groups.
            MessageType::Process => self.gather_group_timestamps(message).await,
        };

        if let Ok(next) = next {
            // Put into mem structure.
            match next {
                NextStep::SendProcesses(m) => self.publisher.send(m).await,
                NextStep::SendGroup(_) => Ok(()),
                NextStep::NoOp => return,
            };
        }
    }

    /// The compute group timestamp step from the specification.
    ///
    /// This method handle messages received through the atomic broadcast primitive.
    async fn compute_group_timestamp(&mut self, _: Message) -> StepResult {
        Ok(NoOp)
    }

    /// The gather group timestamps from the specification.
    ///
    /// This method will execute when at least a single message is received from all partitions.
    /// We must keep track of received message to verify if all partitions already sent their
    /// timestamp. If not all partitions sent the messages yet, we will only insert the received
    /// timestamp and partition in memory and return a no-operation command. This is the only case
    /// where no next step is required.
    ///
    /// This is the only nuance when compared with the specification itself. The remaining code of
    /// this step is a direct translation from the specification.
    async fn gather_group_timestamps(&mut self, _: Message) -> StepResult {
        Ok(NoOp)
    }

    /// The do deliver step, not a direct translation from the specification.
    ///
    /// The specification this method has a guard for execution. In the current implementation this
    /// guard is implemented by the `Mem` structure. This method should be called only when the
    /// message must really be delivered. In the current implementation this method will be called
    /// when the `Mem` structure identify that a message is ready to be delivered.
    ///
    /// We will verify for messages that do not conflict and are also ready to be delivered and we
    /// start delivering all messages.
    async fn do_deliver(&self, _: Message) {}
}

type StepResult = AlgorithmResult<NextStep>;
