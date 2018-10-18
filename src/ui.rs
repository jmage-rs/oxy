#[derive(Default)]
pub struct UiData {
    raw: ::parking_lot::Mutex<Option<::termion::raw::RawTerminal<::std::fs::File>>>,
}

impl crate::oxy::Oxy {
    pub(crate) fn init_ui(&self) {
        assert!(self.in_own_thread());
        self.raw();
        let tty_fd = ::std::os::unix::io::IntoRawFd::into_raw_fd(::termion::get_tty().unwrap());
        let read_token = ::transportation::insert_listener(|_event| {
            crate::oxy::get_oxy().ui_read();
        });
        ::transportation::borrow_poll(|poll| {
            poll.register(
                &::mio::unix::EventedFd(&tty_fd),
                ::mio::Token(read_token),
                ::mio::Ready::readable(),
                ::mio::PollOpt::level(),
            )
            .unwrap()
        });
    }

    fn ui_read(&self) {
        let mut buf = [0u8; 512];
        let mut tty = ::termion::get_tty().unwrap();
        let len = ::std::io::Read::read(&mut tty, &mut buf[..]).unwrap();
        self.info(|| format!("read {:?}", &buf[..len]));
        if &buf[..len] == &[27, 91, 50, 52, 126][..] {
            // F12
            self.cooked();
        }
    }

    pub(crate) fn cooked(&self) {
        self.i.ui.raw.lock().take();
    }

    pub(crate) fn raw(&self) {
        let tty = ::termion::get_tty().unwrap();
        *self.i.ui.raw.lock() = Some(::termion::raw::IntoRawMode::into_raw_mode(tty).unwrap());
    }

    pub(crate) fn tmp_cooked(&self) -> Option<CookGuard> {
        if self.i.ui.raw.lock().take().is_some() {
            return Some(CookGuard { oxy: self.clone() });
        }
        None
    }
}

pub(crate) struct CookGuard {
    oxy: crate::oxy::Oxy,
}

impl ::std::ops::Drop for CookGuard {
    fn drop(&mut self) {
        self.oxy.raw();
    }
}
