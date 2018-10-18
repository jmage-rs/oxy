#[derive(Serialize, Deserialize, Debug)]
pub enum InnerMessage {
    Dummy {},
    ProtocolVersionAnnounce { version: u64 },
    Rekey { new_material: Vec<u8> },
    Reject { message_number: u64, note: String },
    Accept { message_number: u64 },
    Ping {},
    Pong { message_number: u64 },
    PtyRequest { command: Vec<String> },
    PtyInput { input: Vec<u8>, id: u64 },
    PtyOutput { output: Vec<u8>, id: u64 },
}
