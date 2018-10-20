use crate::innermessage::InnerMessage;
use crate::oxy::Oxy;

impl Oxy {
    pub(crate) fn do_metacommand(&self, command: &str) {
        match command {
            "quit" => ::transportation::stop(),
            "pty" => {
                self.send_inner_message(InnerMessage::PtyRequest {
                    path: "/bin/bash".to_string(),
                    argv: ["-bash".to_string()].to_vec(),
                });
            }
            _ => (),
        }
    }
}
