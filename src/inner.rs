pub(crate) fn frame(data: &[u8], mut callback: impl FnMut(&[u8])) {
    let mut this_frame = [0u8; 256];
    let zeros = [0u8; 256];
    for chunk in data.chunks(255) {
        this_frame[0] = ::num::NumCast::from(chunk.len()).unwrap(); // Look, it's statically obvious that an "as" conversion would never be
                                                                    // incorrect, but that's only true until the code changes. Hopefully LLVM
                                                                    // figures out to do this for free. Just let me be paranoid.
        this_frame[1..(1 + chunk.len())].copy_from_slice(chunk);
        this_frame[(1 + chunk.len())..].copy_from_slice(&zeros[..(255 - chunk.len())]);
        callback(&this_frame);
    }
}
