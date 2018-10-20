use crate::innermessage::InnerMessage;
use crate::messagewatchers::{EAT, IGNORE};
use crate::oxy::Oxy;

#[derive(Default)]
pub(crate) struct PtyData {
    ptys: ::parking_lot::Mutex<::std::collections::BTreeMap<u64, Pty>>,
}

struct Pty {
    fd: ::std::os::unix::io::RawFd,
    #[allow(dead_code)]
    pid: ::nix::unistd::Pid,
}

impl Oxy {
    pub(crate) fn new_pty(&self, id: u64) {
        assert!(self.in_own_thread());
        let (pid, fd) = forkpty("/bin/bash", &["bash"]);
        let token =
            ::transportation::insert_listener(move |_event| crate::oxy::get_oxy().notify_pty(id));
        ::transportation::borrow_poll(|poll| {
            poll.register(
                &::mio::unix::EventedFd(&fd),
                ::mio::Token(token),
                ::mio::Ready::readable(),
                ::mio::PollOpt::level(),
            )
            .unwrap();
        });
        let pty = Pty { fd, pid };
        self.i.pty.ptys.lock().insert(id, pty);
        self.watch(move |a, _b, c| match c {
            InnerMessage::PtyInput { input, .. } => {
                let fd = a.i.pty.ptys.lock().get(&id).unwrap().fd;
                ::nix::unistd::write(fd, &input[..]).unwrap();
                EAT
            }
            _ => IGNORE,
        });
    }

    fn notify_pty(&self, id: u64) {
        let mut buf = [0u8; 1024];
        let fd = self.i.pty.ptys.lock().get(&id).unwrap().fd;
        let len = ::nix::unistd::read(fd, &mut buf).unwrap();
        if len != 0 {
            self.send_inner_message(InnerMessage::PtyOutput {
                id,
                output: buf[..len].to_vec(),
            });
        }
    }
}

fn forkpty(path: &str, argv: &[&str]) -> (::nix::unistd::Pid, ::std::os::unix::io::RawFd) {
    use nix::libc::ioctl;
    use nix::libc::TIOCSCTTY;
    use nix::unistd::close;
    use nix::unistd::dup2;
    use nix::unistd::execvp;
    use nix::unistd::fork;
    use nix::unistd::setsid;
    use nix::unistd::ForkResult::{Child, Parent};
    use std::ffi::CString;
    let path = CString::new(path).unwrap();
    let argv: Vec<CString> = argv.iter().map(|x| CString::new(*x).unwrap()).collect();
    let ::nix::pty::OpenptyResult {
        master: parent_fd,
        slave: child_fd,
    } = ::nix::pty::openpty(None, None).unwrap();
    match fork() {
        Ok(Parent { child: child_pid }) => {
            close(child_fd).unwrap();
            return (child_pid, parent_fd);
        }
        Ok(Child) => {
            close(parent_fd).unwrap();
            setsid().unwrap();
            unsafe { ioctl(child_fd, TIOCSCTTY.into(), 0) };
            dup2(child_fd, 0).unwrap();
            dup2(child_fd, 1).unwrap();
            dup2(child_fd, 2).unwrap();
            execvp(&path, &argv[..]).expect("execvp failed");
            unreachable!();
        }
        Err(err) => panic!("Fork failed {:?}", err),
    }
}
