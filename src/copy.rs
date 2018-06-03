use arg;
use byteorder::{self, ByteOrder};
use core::Oxy;
use std::{
    cell::RefCell, fs::{metadata, File}, io::{Read, Write}, net::TcpStream, path::PathBuf, rc::Rc,
};
use transportation::{BufferedTransport, Notifiable, Notifies};

pub fn run() {
    if !arg::homogeneous_sources() {
        eprintln!("Sorry! Copying from multiple different sources isn't supported yet. IT REALLY SHOULD BE. Expect a lot from your tools! Don't let it stay like this forever!");
        ::std::process::exit(1);
    }
    let src = &arg::source_peer_str(0) != "";
    let dest = &arg::dest_peer_str() != "";
    if src && dest {
        remote_to_different_remote();
        #[allow(unreachable_code)]
        {
            unreachable!();
        }
    }
    if src {
        use nix::sys::socket::{socketpair, AddressFamily, SockFlag, SockType};
        let (socka, sockb) = socketpair(AddressFamily::Unix, SockType::Stream, None, SockFlag::empty()).unwrap();
        run_source(socka.into());
        let bt: BufferedTransport = sockb.into();
        let bt2 = bt.clone();
        let service = RecvFilesService {
            bt,
            file: Rc::new(RefCell::new(None)),
            id: Rc::new(RefCell::new(0)),
        };
        bt2.set_notify(Rc::new(service));
        ::transportation::run();
    }
    if dest {
        use nix::sys::socket::{socketpair, AddressFamily, SockFlag, SockType};
        let (socka, sockb) = socketpair(AddressFamily::Unix, SockType::Stream, None, SockFlag::empty()).unwrap();
        run_dest(socka.into());
        let bt: BufferedTransport = sockb.into();
        let bt2 = bt.clone();
        let service = SendFilesService {
            bt,
            file: Rc::new(RefCell::new(None)),
            id: Rc::new(RefCell::new(0)),
        };
        let service = Rc::new(service);
        bt2.set_notify(service.clone());
        service.notify();
        ::transportation::run();
    }
    warn!(
        "You appear to be asking me to copy local files to a local destination. \
         I mean, I'll do it for you, but it seems like a weird thing to ask of a remote access tool."
    );
    let dest = arg::matches().value_of("dest").unwrap();
    let metadata = ::std::fs::metadata(&dest);
    let dir = metadata.is_ok() && metadata.unwrap().is_dir();
    for source in arg::matches().values_of("source").unwrap() {
        let source: PathBuf = source.into();
        let source: PathBuf = source.canonicalize().unwrap();
        let dest2: PathBuf = dest.into();
        let mut dest2: PathBuf = dest2.canonicalize().unwrap();
        if dir {
            dest2.push(source.file_name().unwrap());
        }
        let result = ::std::fs::copy(&source, &dest2);
        if result.is_err() {
            warn!("{:?}", result);
        }
    }
}

fn remote_to_different_remote() -> ! {
    use nix::sys::socket::{socketpair, AddressFamily, SockFlag, SockType};
    let (socka, sockb) = socketpair(AddressFamily::Unix, SockType::Stream, None, SockFlag::empty()).unwrap();
    // ^ This is the only thing that's not Windows compatible about this at this
    // point... Gotta replace it with a "null" BufferedTransport or something.
    run_dest(socka.into());
    run_source(sockb.into());
    ::transportation::run();
}

fn run_source(peer: BufferedTransport) {
    let dest = arg::source_peer(0);
    let remote = TcpStream::connect(&dest[..]).unwrap();
    let oxy = Oxy::create(remote);
    oxy.fetch_files(peer);
    oxy.soft_launch();
}

fn run_dest(peer: BufferedTransport) {
    let dest = arg::dest_peer();
    let remote = TcpStream::connect(&dest[..]).unwrap();
    let oxy = Oxy::create(remote);
    oxy.recv_files(peer);
    oxy.soft_launch();
}

struct RecvFilesService {
    bt:   BufferedTransport,
    file: Rc<RefCell<Option<File>>>,
    id:   Rc<RefCell<u64>>,
}

impl Notifiable for RecvFilesService {
    fn notify(&self) {
        for msg in self.bt.recv_all_messages() {
            let number = byteorder::BE::read_u64(&msg[..8]);
            if number == ::std::u64::MAX {
                ::std::process::exit(0);
            }
            if self.file.borrow().is_none() || number != *self.id.borrow() {
                if self.file.borrow().is_none() {
                    assert!(number == 0);
                }
                let mut path: PathBuf = arg::dest_path().into();
                let metadata = metadata(&path);
                if metadata.is_ok() && metadata.unwrap().is_dir() {
                    let part: PathBuf = arg::source_path(number).into();
                    let part = part.canonicalize().unwrap();
                    path.push(part.file_name().unwrap());
                }
                let file = File::create(path).unwrap();
                *self.file.borrow_mut() = Some(file);
                self.file.borrow_mut().as_mut().unwrap().write_all(&msg[8..]).unwrap();
            }
        }
    }
}

struct SendFilesService {
    bt:   BufferedTransport,
    file: Rc<RefCell<Option<File>>>,
    id:   Rc<RefCell<u64>>,
}

impl Notifiable for SendFilesService {
    fn notify(&self) {
        if self.file.borrow().is_none() {
            let id = *self.id.borrow();
            trace!("ID: {:?}", id);
            if id == ::std::u64::MAX {
                return;
            }
            if id >= arg::matches().occurrences_of("source") {
                self.bt.send_message(b"\xff\xff\xff\xff\xff\xff\xff\xff");
                *self.id.borrow_mut() = ::std::u64::MAX;
                return;
            }
            *self.file.borrow_mut() = Some(File::open(arg::source_path(id)).unwrap());
        }
        if self.bt.has_write_space() {
            let mut page = [0u8; 1024];
            let result = self.file.borrow_mut().as_mut().unwrap().read(&mut page);
            info!("{:?}", result);
            if result.is_ok() {
                let mut message: Vec<u8> = Vec::new();
                message.resize(8, 0);
                byteorder::BE::write_u64(&mut message[..8], *self.id.borrow());
                message.extend(&page[..*result.as_ref().unwrap()]);
                self.bt.send_message(&message);
            }
            if result.is_err() || result.unwrap() < page.len() {
                self.file.borrow_mut().take();
                *self.id.borrow_mut() += 1;
            }
        }
    }
}