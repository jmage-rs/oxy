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
    pocket_thread_id: ::parking_lot::Mutex<Option<::std::thread::ThreadId>>,
    outbound_mid_sequence_number: ::parking_lot::Mutex<u32>,
    pub(crate) destination: ::parking_lot::Mutex<Option<::std::net::SocketAddr>>,
    conversations: ::parking_lot::Mutex<::std::collections::BTreeMap<u64, Oxy>>,
    conversation_id_ticker: ::parking_lot::Mutex<u64>,
    incoming_frame_buffer: ::parking_lot::Mutex<::arrayvec::ArrayVec<[[u8; 255]; 64]>>,
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

    fn new_deferred(config: crate::config::Config) -> Oxy {
        let result: Oxy = Default::default();
        *result.i.config.lock() = config.clone();
        result
    }

    fn init(&self) {
        match self.mode() {
            crate::config::Mode::Server => self.init_server(),
            crate::config::Mode::Client => self.init_client(),
            crate::config::Mode::ServerConnection => self.init_server_connection(),
        }
    }

    fn init_server_connection(&self) {
        let noise = ::snow::Builder::new(crate::NOISE_PATTERN.parse().unwrap())
            .local_private_key(&self.local_private_key())
            .remote_public_key(&self.remote_public_key())
            .build_responder()
            .unwrap();
        *self.i.noise.lock() = Some(noise);
        self.spawn_pocket_thread();
    }

    fn recv_inner(&self, inner: &[u8]) {
        if inner[0] == 255 {
            let mut frame = [0u8; 255];
            frame.copy_from_slice(&inner[1..]);
            self.i.incoming_frame_buffer.lock().push(frame);
            return;
        }
        let mut buf = [0u8; 16574];
        let mut caret = 0usize;
        for i in self.i.incoming_frame_buffer.lock().iter() {
            buf[caret..caret + 255].copy_from_slice(&i[..]);
            caret += 255;
        }
        self.i.incoming_frame_buffer.lock().clear();
        buf[caret..(caret + (inner[0] as usize))]
            .copy_from_slice(&inner[1..(1 + (inner[0] as usize))]);
        caret += inner[0] as usize;
        self.recv_inner_full(&buf[..caret]);
    }

    fn recv_inner_full(&self, message: &[u8]) {
        let message: crate::innermessage::InnerMessage = ::serde_cbor::from_slice(message).unwrap();
        println!("{:?}", message);
    }

    fn recv_mid(&self, mid: &[u8]) {
        let mut buf = [0u8; 1024];
        let mut noise_lock = self.i.noise.lock();
        if noise_lock.as_ref().unwrap().is_handshake_finished() {
            let noise = noise_lock.as_mut().unwrap();
            let len = noise
                .read_message(&crate::mid::get_payload(mid), &mut buf)
                .unwrap();
            assert!(len == 256);
            self.recv_inner(&buf[..256]);
        } else {
            let mut noise = noise_lock.take().unwrap();
            let payload = crate::mid::get_payload(mid);
            assert!(payload[0] == 96 || payload[0] == 48);
            let frameend: usize = 1 + (payload[0] as usize);
            let mut len = noise
                .read_message(&crate::mid::get_payload(mid)[1..frameend], &mut buf)
                .unwrap();
            assert!(len == 0);
            if !noise.is_initiator() {
                len = noise.write_message(b"", &mut buf).unwrap();
                self.send_inner_packet_fake_tag(&buf[..len]);
            }
            let noise = noise.into_transport_mode().unwrap();
            *noise_lock = Some(noise);
            self.info(|| "Handshake completed");
            self.send_inner_message(crate::innermessage::InnerMessage::Dummy {});
        }
    }

    fn send_inner_message(&self, message: crate::innermessage::InnerMessage) {
        let mut message_buf = [0u8; 16574];
        let len: usize;
        {
            let mut message_buf_cursor = ::std::io::Cursor::new(&mut message_buf[..]);
            ::serde_cbor::to_writer(&mut message_buf_cursor, &message).unwrap();
            len = ::num::NumCast::from(message_buf_cursor.position()).unwrap();
        }
        self.send_inner_packet(&message_buf[..len]);
    }

    /// Wait for this oxy instance to finish.
    pub fn join(&self) {
        // If multiple threads try and join on the same instance, they'll all wind up
        // waiting for the mutex for the joinhandle? So it kinda works out?
        if let Some(thread) = self.i.pocket_thread.lock().take() {
            thread.join().unwrap();
        }
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
        let mut pocket_thread_lock = self.i.pocket_thread.lock();
        let mut pocket_thread_id_lock = self.i.pocket_thread_id.lock();
        assert!(pocket_thread_lock.is_none());
        let pocket_thread = ::std::thread::spawn(|| transportation::run_worker());
        let pocket_thread_id = pocket_thread.thread().id();
        *pocket_thread_lock = Some(pocket_thread);
        *pocket_thread_id_lock = Some(pocket_thread_id);
        let proxy = self.clone();
        ::transportation::run_in_thread(pocket_thread_id, move || proxy.init_from_pocket_thread())
            .unwrap();
    }

    fn init_from_pocket_thread(&self) {
        match self.mode() {
            crate::config::Mode::Server => self.init_server_from_pocket_thread(),
            crate::config::Mode::Client => self.init_client_from_pocket_thread(),
            crate::config::Mode::ServerConnection => (),
        }
    }

    fn send_inner_packet(&self, payload: &[u8]) {
        crate::inner::frame(payload, |mid| {
            let mut buf = [0u8; 272];
            let len = self
                .i
                .noise
                .lock()
                .as_mut()
                .unwrap()
                .write_message(&mid, &mut buf[..])
                .unwrap();
            assert!(len == 272);
            self.send_mid_packet(&buf[..]);
        });
    }

    fn send_inner_packet_fake_tag(&self, payload: &[u8]) {
        let mut fake = [0u8; 272];
        crate::inner::frame(payload, |mid| {
            fake[..256].copy_from_slice(mid);
            self.send_mid_packet(&fake);
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
        let weak = self.downgrade();
        let token = ::transportation::insert_listener(move |event| match weak.upgrade() {
            Some(oxy) => oxy.socket_event(event),
            None => {
                transportation::stop();
            }
        });
        ::transportation::borrow_poll(|poll| {
            poll.register(
                self.i.socket.lock().as_ref().unwrap(),
                ::mio::Token(token),
                ::mio::Ready::readable(),
                ::mio::PollOpt::edge(),
            )
            .unwrap()
        });
        let mut buf = [0u8; 1024];
        let len = self
            .i
            .noise
            .lock()
            .as_mut()
            .unwrap()
            .write_message(b"", &mut buf)
            .unwrap();
        self.send_inner_packet_fake_tag(&buf[..len]);
    }

    fn init_server_from_pocket_thread(&self) {
        let port = self.port_number();
        let ip: ::std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let bind_addr = ::std::net::SocketAddr::new(ip, port);
        let socket = ::mio::net::UdpSocket::bind(&bind_addr).unwrap();
        self.info(|| "Successfully bound server socket");
        let weak = self.downgrade();
        let token = ::transportation::insert_listener(move |event| match weak.upgrade() {
            Some(oxy) => oxy.socket_event(event),
            None => {
                transportation::stop();
            }
        });
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

    fn init_server(&self) {
        self.spawn_pocket_thread();
    }

    fn mode(&self) -> crate::config::Mode {
        self.i.config.lock().mode.expect("Oxy instance lacks mode")
    }

    fn socket_event(&self, event: ::mio::Event) {
        if event.readiness().is_readable() {
            self.read_socket();
        }
    }

    fn process_mid_packet(&self, src: ::std::net::SocketAddr, mid: &[u8]) {
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
                    let worker = Oxy::new_deferred(new_config);
                    *worker.i.socket.lock() =
                        Some(self.i.socket.lock().as_ref().unwrap().try_clone().unwrap());
                    *worker.i.destination.lock() = Some(src);
                    worker.init();
                    worker.recv_mid(mid);
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
                self.recv_mid(mid);
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
            let result = self.i.socket.lock().as_ref().unwrap().recv_from(&mut buf);
            match result {
                Ok((amt, src)) => {
                    if amt != crate::outer::OUTER_PACKET_SIZE {
                        self.warn(|| "Read less than one message worth in one call.");
                        continue;
                    }
                    let decrypt = self.decrypt_outer_packet(&buf, |mid| {
                        self.info(|| "Got a mid packet");
                        self.process_mid_packet(src, mid);
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
