#[derive(Serialize, Deserialize, Debug)]
pub enum InnerMessage {
    Dummy {},
    ProtocolVersionAnnounce { version: u64 },
    Rekey { new_material: Vec<u8> },
    Reject { message_number: u64, note: String },
    Accept { message_number: u64 },
    Ping {},
    Pong { message_number: u64 },
    PtyRequest { path: String, argv: Vec<String> },
    PtyInput { id: u64, input: Vec<u8> },
    PtyOutput { id: u64, output: Vec<u8> },
    PtySizeAdvertisement { id: u64, w: u16, h: u16 },
}
