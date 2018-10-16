//! This module contains the main data structure for an Oxy connection.

/// The main data structure for an Oxy connection. This data structure is Send
/// + Sync and internally mutable.
#[derive(Default, Clone)]
pub struct Oxy {
    pub(crate) i: ::std::sync::Arc<OxyInternal>,
}

#[derive(Default)]
pub(crate) struct OxyInternal {
    pub(crate) config: ::parking_lot::Mutex<crate::config::Config>,
    socket: ::parking_lot::Mutex<Option<::mio::net::UdpSocket>>,
    socket_token: ::parking_lot::Mutex<Option<usize>>,
    noise: ::parking_lot::Mutex<Option<::snow::Session>>,
    pocket_thread: ::parking_lot::Mutex<Option<::std::thread::JoinHandle<()>>>,
    outbound_mid_sequence_number: ::parking_lot::Mutex<u32>,
    pub(crate) destination: ::parking_lot::Mutex<Option<::std::net::SocketAddr>>,
    conversations: ::parking_lot::Mutex<::std::collections::BTreeMap<u64, Oxy>>,
    conversation_id_ticker: ::parking_lot::Mutex<u64>,
}

impl Oxy {
    /// Make a weak reference to this Oxy instance.
    pub fn downgrade(&self) -> OxyWeak {
        OxyWeak {
            i: ::std::sync::Arc::downgrade(&self.i),
        }
    }

    /// Create a new Oxy instance with a provided config.
    pub fn new(config: crate::config::Config) -> Oxy {
        let result: Oxy = Default::default();
        *result.i.config.lock() = config.clone();
        result.init();
        result
    }

    fn init(&self) {
        match self.mode() {
            crate::config::Mode::Server => self.init_server(),
            crate::config::Mode::Client => self.init_client(),
            crate::config::Mode::ServerConnection => (),
        }
    }

    fn recv_mid(&self, mid: &[u8]) {
        let mut buf = [0u8; 1024];
        let len = self
            .i
            .noise
            .lock()
            .as_mut()
            .unwrap()
            .read_message(crate::mid::get_payload(mid), &mut buf)
            .unwrap();
        self.info(|| format!("Got {:?}", &buf[..len]));
    }

    fn init_client(&self) {
        let destination = self.i.config.lock().destination.as_ref().unwrap().clone();
        *self.i.destination.lock() = Some(
            ::std::net::ToSocketAddrs::to_socket_addrs(&destination)
                .expect("failed to resolve destination")
                .next()
                .expect("no address for destination"),
        );

        *self.i.noise.lock() = Some(
            ::snow::Builder::new(crate::NOISE_PATTERN.parse().unwrap())
                .local_private_key(&self.local_private_key())
                .remote_public_key(&self.remote_public_key())
                .build_initiator()
                .unwrap(),
        );
        let socket = ::mio::net::UdpSocket::bind(&"0.0.0.0:0".parse().unwrap()).unwrap();
        *self.i.socket.lock() = Some(socket);
        self.spawn_pocket_thread();
    }

    fn spawn_pocket_thread(&self) {
        let pocket_thread = ::std::thread::spawn(|| transportation::run_worker());
        let pocket_thread_id = pocket_thread.thread().id();
        *self.i.pocket_thread.lock() = Some(pocket_thread);
        let proxy = self.clone();
        ::transportation::run_in_thread(pocket_thread_id, move || proxy.init_from_pocket_thread())
            .unwrap();
    }

    fn init_from_pocket_thread(&self) {
        match self.mode() {
            crate::config::Mode::Server => self.init_server_from_pocket_thread(),
            crate::config::Mode::Client => self.init_client_from_pocket_thread(),
            _ => unimplemented!(),
        }
    }

    fn send_inner_packet(&self, payload: &[u8]) {
        crate::inner::frame(payload, |mid| {
            self.send_mid_packet(mid);
        });
    }

    fn send_mid_packet(&self, payload: &[u8]) {
        let mut packet = [0u8; crate::outer::MID_PACKET_SIZE];
        let sequence_number = {
            let mut lock = self.i.outbound_mid_sequence_number.lock();
            let mine = *lock;
            let next = lock.checked_add(1).unwrap();
            *lock = next;
            mine
        };
        crate::mid::set_conversation_id(&mut packet, 0);
        crate::mid::set_sequence_number(&mut packet, sequence_number);
        crate::mid::set_acknowledgement_number(&mut packet, 0);
        crate::mid::set_buffer_size(&mut packet, 0);
        crate::mid::get_payload_mut(&mut packet).copy_from_slice(payload);
        self.encrypt_outer_packet(&packet, |outer| {
            self.i
                .socket
                .lock()
                .as_mut()
                .unwrap()
                .send_to(outer, &self.destination())
        })
        .unwrap();
    }

    fn init_client_from_pocket_thread(&self) {
        let mut buf = [0u8; 1024];
        let len = self
            .i
            .noise
            .lock()
            .as_mut()
            .unwrap()
            .write_message(b"", &mut buf)
            .unwrap();
        self.send_inner_packet(&buf[..len]);
    }

    fn init_server_from_pocket_thread(&self) {}

    fn init_server(&self) {
        let socket = ::mio::net::UdpSocket::bind(&"127.0.0.1:2600".parse().unwrap()).unwrap();
        self.info(|| "Successfully bound server socket");
        let weak = self.downgrade();
        let token_holder = ::std::rc::Rc::new(::std::cell::RefCell::new(0));
        let token_holder2 = token_holder.clone();
        let token = ::transportation::insert_listener(move |event| match weak.upgrade() {
            Some(oxy) => oxy.socket_event(event),
            None => {
                transportation::remove_listener(*token_holder.borrow());
            }
        });
        *token_holder2.borrow_mut() = token;
        ::transportation::borrow_poll(|poll| {
            poll.register(
                &socket,
                ::mio::Token(token),
                ::mio::Ready::readable(),
                ::mio::PollOpt::edge(),
            )
            .unwrap()
        });
        *self.i.socket.lock() = Some(socket);
        *self.i.socket_token.lock() = Some(token);
    }

    fn mode(&self) -> crate::config::Mode {
        self.i.config.lock().mode.expect("Oxy instance lacks mode")
    }

    fn socket_event(&self, event: ::mio::Event) {
        if event.readiness().is_readable() {
            self.read_socket();
        }
    }

    fn process_mid_packet(&self, mid: &[u8]) {
        match self.mode() {
            crate::config::Mode::Server => {
                self.info(|| {
                    format!(
                        "Server processing mid packet: {:?}, {:?}, {:?}, {:?}, {:?}",
                        crate::mid::get_conversation_id(mid),
                        crate::mid::get_sequence_number(mid),
                        crate::mid::get_acknowledgement_number(mid),
                        crate::mid::get_buffer_size(mid),
                        crate::mid::get_payload(mid)
                    )
                });
                let conversation_id = crate::mid::get_conversation_id(mid);
                if conversation_id == 0 {
                    let conversation_id = self.make_conversation_id();
                    let mut new_config = self.i.config.lock().clone();
                    new_config.mode = Some(crate::config::Mode::ServerConnection);
                    let worker = Oxy::new(new_config);
                    self.i.conversations.lock().insert(conversation_id, worker);
                } else {
                    if let Some(worker) = self.i.conversations.lock().get_mut(&conversation_id) {
                        worker.recv_mid(mid);
                    } else {
                        self.warn(|| {
                            format!("Mid message for unknown conversation {}", conversation_id)
                        });
                    }
                }
            }
            crate::config::Mode::Client => {
                // Feed the packet into the noise session
            }
            _ => unimplemented!(),
        }
    }

    fn make_conversation_id(&self) -> u64 {
        // IDK, I was thinking about having random conversation IDs as a hardening
        // thing, but I'm not sure it actually buys anything. I'll come back to this
        // later.
        let mut lock = self.i.conversation_id_ticker.lock();
        let mine = *lock;
        let next = lock.checked_add(1).unwrap();
        *lock = next;
        mine
    }

    fn read_socket(&self) {
        loop {
            let mut buf = [0u8; crate::outer::OUTER_PACKET_SIZE];
            match self.i.socket.lock().as_ref().unwrap().recv_from(&mut buf) {
                Ok((amt, _src)) => {
                    if amt != crate::outer::OUTER_PACKET_SIZE {
                        self.warn(|| "Read less than one message worth in one call.");
                        continue;
                    }
                    let decrypt = self.decrypt_outer_packet(&buf, |mid| {
                        self.process_mid_packet(mid);
                    });
                    if decrypt.is_err() {
                        self.warn(|| "Rejecting packet with bad tag.");
                    };
                }
                Err(err) => {
                    if err.kind() == ::std::io::ErrorKind::WouldBlock {
                        break;
                    }
                    self.warn(|| format!("Error reading socket: {:?}", err));
                }
            }
        }
    }
}

/// A weak reference counted handle to an Oxy instance. Used to break reference
/// cycles. Only useful for upgrading to a real instance.
pub struct OxyWeak {
    i: ::std::sync::Weak<OxyInternal>,
}

impl OxyWeak {
    /// Upgrade to a real Oxy instance (if the corresponding real Oxy still
    /// exists)
    pub fn upgrade(&self) -> Option<Oxy> {
        match self.i.upgrade() {
            Some(i) => Some(Oxy { i }),
            None => None,
        }
    }
}
