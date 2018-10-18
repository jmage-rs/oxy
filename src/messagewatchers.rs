impl crate::oxy::Oxy {
    pub(crate) fn log_message(
        &self,
        id: u64,
        message: &crate::innermessage::InnerMessage,
    ) -> crate::oxy::MessageWatcherResult {
        self.info(|| format!("Processing message: {} {:?}", id, message));
        crate::oxy::MessageWatcherResult {
            keep_watching: true,
            consume_message: false,
        }
    }

    pub(crate) fn pong(
        &self,
        id: u64,
        message: &crate::innermessage::InnerMessage,
    ) -> crate::oxy::MessageWatcherResult {
        match message {
            crate::innermessage::InnerMessage::Ping {} => {
                self.send_inner_message(crate::innermessage::InnerMessage::Pong {
                    message_number: id,
                });
                crate::oxy::MessageWatcherResult {
                    keep_watching: true,
                    consume_message: true,
                }
            }
            _ => crate::oxy::MessageWatcherResult {
                keep_watching: true,
                consume_message: false,
            },
        }
    }
}
