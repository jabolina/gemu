use tokio::sync::broadcast;

/// Tells whether some structure was shutdown.
///
/// This structure receives a [`broadcast::Receiver`] as an argument that receives only a single
/// value, after the value is received the structure will shutdown.
pub(crate) struct Shutdown {
    // This will be set to `true` once the `killed` method is called.
    shutdown: bool,

    // Channel to receive the notification about the structure death.
    rx: broadcast::Receiver<()>,
}

impl Shutdown {
    /// Create new [`Shutdown`] structure that will listen to the given channel.
    pub(crate) fn new(rx: broadcast::Receiver<()>) -> Self {
        Shutdown {
            shutdown: false,
            rx,
        }
    }

    /// Verify if it was shutdown.
    pub(crate) fn is_shut(&self) -> bool {
        self.shutdown
    }

    /// Try to shutdown the current structure, this can only be done if a signal is received
    /// through the channel.
    pub(crate) async fn wait_shutdown(&mut self) {
        if self.shutdown {
            return;
        }

        // Does not matter what is the result, only that a result has happened at all.
        let _ = self.rx.recv().await;
        self.shutdown = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::shutdown::Shutdown;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn should_be_on_until_shutdown() {
        let (tx, _) = broadcast::channel(1);
        let mut alive = Shutdown::new(tx.subscribe());

        assert!(alive.is_shut());

        let shut = tokio::spawn(async move {
            assert!(tx.send(()).is_ok());
        });

        alive.wait_shutdown().await;

        assert!(!alive.is_shut());
        assert!(shut.await.is_ok());
    }
}
