#[derive(Default)]
pub(crate) struct PtyData {
    ptys: ::parking_lot::Mutex<::std::collections::BTreeMap<u64, Pty>>,
}

struct Pty {
    fd: ::std::os::unix::io::RawFd,
}
