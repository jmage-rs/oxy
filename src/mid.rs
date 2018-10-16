pub(crate) fn get_conversation_id(data: &[u8]) -> u64 {
    let mut conversation_id = [0u8; 8];
    conversation_id.copy_from_slice(&data[..8]);
    u64::from_le_bytes(conversation_id)
}

pub(crate) fn set_conversation_id(data: &mut [u8], id: u64) {
    data[..8].copy_from_slice(&id.to_le_bytes()[..]);
}

pub(crate) fn get_sequence_number(data: &[u8]) -> u32 {
    let mut sequence_number = [0u8; 4];
    sequence_number.copy_from_slice(&data[8..12]);
    u32::from_le_bytes(sequence_number)
}

pub(crate) fn set_sequence_number(data: &mut [u8], sequence_number: u32) {
    data[8..12].copy_from_slice(&sequence_number.to_le_bytes()[..])
}

pub(crate) fn get_acknowledgement_number(data: &[u8]) -> u32 {
    let mut acknowledgement_number = [0u8; 4];
    acknowledgement_number.copy_from_slice(&data[12..18]);
    u32::from_le_bytes(acknowledgement_number)
}

pub(crate) fn set_buffer_size(data: &mut [u8], buffer_size: u16) {
    data[18..20].copy_from_slice(&buffer_size.to_le_bytes()[..])
}

pub(crate) fn get_buffer_size(data: &[u8]) -> u16 {
    let mut buffer_size = [0u8; 2];
    buffer_size.copy_from_slice(&data[18..20]);
    u16::from_le_bytes(buffer_size)
}

pub(crate) fn get_payload<'a>(data: &'a [u8]) -> &'a [u8] {
    &data[20..292]
}

pub(crate) fn get_payload_mut<'a>(data: &'a mut [u8]) -> &'a mut [u8] {
    &mut data[20..292]
}

