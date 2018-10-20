use crate::innermessage::InnerMessage;
use crate::oxy::MessageWatcherResult;

pub(crate) const DONE: MessageWatcherResult = MessageWatcherResult {
    keep_watching: false,
    consume_message: true,
};

pub(crate) const IGNORE: MessageWatcherResult = MessageWatcherResult {
    keep_watching: true,
    consume_message: false,
};

pub(crate) const EAT: MessageWatcherResult = MessageWatcherResult {
    keep_watching: true,
    consume_message: true,
};

impl crate::oxy::Oxy {
    pub(crate) fn log_message(&self, id: u64, message: &InnerMessage) -> MessageWatcherResult {
        self.info(|| format!("Processing message: {} {:?}", id, message));
        IGNORE
    }

    pub(crate) fn pong(&self, id: u64, message: &InnerMessage) -> MessageWatcherResult {
        match message {
            crate::innermessage::InnerMessage::Ping {} => {
                self.send_inner_message(InnerMessage::Pong { message_number: id });
                EAT
            }
            _ => IGNORE,
        }
    }
}
