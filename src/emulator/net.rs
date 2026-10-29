//! Implements the application/game UDP networking protocol.
//! Tracks player slots, peer discovery and timeouts, packet exchange, and receive queues;
//! this protocol is separate from SC-NET.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 47_016;
pub const MAX_PLAYERS: u8 = 8;
pub const MAX_PAYLOAD: usize = 48;
#[cfg(not(target_arch = "wasm32"))]
const HEADER_SIZE: usize = 26;
#[cfg(not(target_arch = "wasm32"))]
const HELLO: u8 = 1;
const DATA: u8 = 2;
const BYE: u8 = 3;
#[cfg(not(target_arch = "wasm32"))]
const JOIN_INTERVAL: Duration = Duration::from_millis(200);
const JOIN_TIME: Duration = Duration::from_millis(600);
#[cfg(not(target_arch = "wasm32"))]
const HELLO_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(target_arch = "wasm32"))]
const PEER_TIMEOUT: Duration = Duration::from_millis(3500);
#[cfg(not(target_arch = "wasm32"))]
const INBOX_LIMIT: usize = 64;

#[derive(Clone)]
struct Peer {
    slot: u8,
    #[cfg(not(target_arch = "wasm32"))]
    last_seen: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    settled: bool,
}

pub struct NetNode {
    #[cfg(not(target_arch = "wasm32"))]
    instance_id: u32,
    #[cfg(not(target_arch = "wasm32"))]
    port: u16,
    title: Option<[u8; 16]>,
    slot: Option<u8>,
    full: bool,
    joined_at: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    last_hello: Instant,
    peers: HashMap<u32, Peer>,
    inbox: VecDeque<(u8, Vec<u8>)>,
    last_sender: u8,
    #[cfg(not(target_arch = "wasm32"))]
    socket: Option<std::net::UdpSocket>,
}

impl NetNode {
    pub fn new(_port: u16) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            instance_id: make_instance_id(),
            #[cfg(not(target_arch = "wasm32"))]
            port: _port,
            title: None,
            slot: None,
            full: false,
            joined_at: Instant::now(),
            #[cfg(not(target_arch = "wasm32"))]
            last_hello: Instant::now(),
            peers: HashMap::new(),
            inbox: VecDeque::new(),
            last_sender: 0,
            #[cfg(not(target_arch = "wasm32"))]
            socket: None,
        }
    }

    pub fn open(&mut self, title: &[u8]) -> Result<bool, String> {
        self.close();
        if title.iter().all(|byte| *byte == 0) {
            return Ok(false);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = title;
            return Err("UDP networking is unavailable in the browser runtime".to_owned());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ensure_socket()?;
            let mut padded_title = [0; 16];
            let len = title.len().min(padded_title.len());
            padded_title[..len].copy_from_slice(&title[..len]);
            self.title = Some(padded_title);
            self.peers.clear();
            self.inbox.clear();
            self.full = false;
            self.slot = Some(0);
            self.joined_at = Instant::now();
            self.last_hello = self.joined_at;
            self.send_packet(HELLO, &[0]);
            Ok(true)
        }
    }

    pub fn close(&mut self) {
        if self.title.is_some() && self.slot.is_some() {
            self.send_packet(BYE, &[]);
        }
        self.title = None;
        self.slot = None;
        self.peers.clear();
        self.inbox.clear();
    }

    pub fn poll(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.title.is_none() {
                return;
            }
            loop {
                let mut packet = [0; 1024];
                let received =
                    match self.socket.as_ref().and_then(|socket| {
                        socket.recv_from(&mut packet).ok().map(|(length, _)| length)
                    }) {
                        Some(length) => length,
                        None => break,
                    };
                self.handle_packet(&packet[..received], Instant::now());
            }
            let now = Instant::now();
            self.peers
                .retain(|_, peer| now.duration_since(peer.last_seen) <= PEER_TIMEOUT);
            if self.slot.is_some() {
                let joining = now.duration_since(self.joined_at) < JOIN_TIME;
                let interval = if joining {
                    JOIN_INTERVAL
                } else {
                    HELLO_INTERVAL
                };
                if now.duration_since(self.last_hello) >= interval {
                    self.send_hello();
                }
            }
        }
    }

    pub fn send(&mut self, payload: &[u8]) -> bool {
        if self.slot.is_none() || payload.len() > MAX_PAYLOAD {
            return false;
        }
        self.send_packet(DATA, payload);
        true
    }

    pub fn try_send(&self, payload: &[u8]) -> Result<bool, String> {
        if self.slot.is_none() || payload.len() > MAX_PAYLOAD {
            return Ok(false);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.try_send_packet_to(
                DATA,
                payload,
                (std::net::Ipv4Addr::BROADCAST, self.port).into(),
            )?;
            Ok(true)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("UDP networking is unavailable in the browser runtime".to_owned())
        }
    }

    pub fn receive(&mut self) -> Option<(u8, Vec<u8>)> {
        self.poll();
        let message = self.inbox.pop_front()?;
        self.last_sender = message.0;
        Some(message)
    }

    pub fn players(&self) -> u16 {
        if self.slot.is_some() {
            (self.peers.len() + 1) as u16
        } else {
            0
        }
    }

    pub fn slot(&self) -> i16 {
        self.slot.map_or(-1, i16::from)
    }

    pub fn active(&self, slot: u16) -> bool {
        if slot >= u16::from(MAX_PLAYERS) || self.slot.is_none() {
            return false;
        }
        slot == u16::from(self.slot.unwrap_or_default())
            || self.peers.values().any(|peer| u16::from(peer.slot) == slot)
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn last_sender(&self) -> u16 {
        u16::from(self.last_sender)
    }

    pub fn ready(&self) -> bool {
        self.slot.is_some() && Instant::now().duration_since(self.joined_at) >= JOIN_TIME
    }

    pub fn is_open(&self) -> bool {
        self.title.is_some()
    }

    pub fn query(&self, query: u8, argument: u16) -> Option<u16> {
        match query {
            0 => Some(self.players()),
            1 => Some(self.slot() as u16),
            2 => Some(u16::from(self.active(argument))),
            3 => Some(u16::from(self.is_full())),
            4 => Some(self.last_sender()),
            5 => Some(u16::from(self.ready())),
            6 => Some(u16::from(self.is_open())),
            _ => None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn settled(&self, now: Instant) -> bool {
        self.slot.is_some() && now.duration_since(self.joined_at) >= JOIN_TIME
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn free_slot(&self) -> Option<u8> {
        (0..MAX_PLAYERS).find(|slot| !self.peers.values().any(|peer| peer.slot == *slot))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send_hello(&mut self) {
        self.last_hello = Instant::now();
        self.send_packet(HELLO, &[u8::from(self.settled(self.last_hello))]);
    }

    fn send_packet(&self, kind: u8, payload: &[u8]) {
        #[cfg(not(target_arch = "wasm32"))]
        self.send_packet_to(
            kind,
            payload,
            (std::net::Ipv4Addr::BROADCAST, self.port).into(),
        );
        #[cfg(target_arch = "wasm32")]
        let _ = (kind, payload);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send_packet_to(&self, kind: u8, payload: &[u8], destination: std::net::SocketAddr) {
        let _ = self.try_send_packet_to(kind, payload, destination);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn try_send_packet_to(
        &self,
        kind: u8,
        payload: &[u8],
        destination: std::net::SocketAddr,
    ) -> Result<(), String> {
        if let (Some(socket), Some(title)) = (&self.socket, self.title) {
            let mut packet = Vec::with_capacity(HEADER_SIZE + payload.len());
            packet.extend_from_slice(b"SC16");
            packet.push(kind);
            packet.push(self.slot.unwrap_or(0));
            packet.extend_from_slice(&title);
            packet.extend_from_slice(&self.instance_id.to_be_bytes());
            packet.extend_from_slice(payload);
            let sent = socket
                .send_to(&packet, destination)
                .map_err(|error| format!("cannot send UDP packet: {error}"))?;
            if sent != packet.len() {
                return Err("UDP packet was not sent in full".to_owned());
            }
            Ok(())
        } else {
            Err("UDP socket is not open".to_owned())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_socket(&mut self) -> Result<(), String> {
        if self.socket.is_some() {
            return Ok(());
        }
        use socket2::{Domain, Protocol, SockAddr, Socket, Type};
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|error| format!("cannot create UDP socket: {error}"))?;
        socket
            .set_reuse_address(true)
            .map_err(|error| format!("cannot configure UDP socket: {error}"))?;
        socket
            .set_broadcast(true)
            .map_err(|error| format!("cannot enable UDP broadcast: {error}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure UDP socket: {error}"))?;
        socket
            .bind(&SockAddr::from(std::net::SocketAddr::from((
                [0, 0, 0, 0],
                self.port,
            ))))
            .map_err(|error| format!("cannot bind UDP port {}: {error}", self.port))?;
        self.socket = Some(socket.into());
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_packet(&mut self, data: &[u8], now: Instant) {
        if data.len() < HEADER_SIZE || &data[..4] != b"SC16" {
            return;
        }
        let kind = data[4];
        let slot = data[5];
        let mut title = [0; 16];
        title.copy_from_slice(&data[6..22]);
        let sender = u32::from_be_bytes([data[22], data[23], data[24], data[25]]);
        if Some(title) != self.title || sender == self.instance_id || slot >= MAX_PLAYERS {
            return;
        }
        if kind == BYE {
            self.peers.remove(&sender);
            return;
        }
        let payload = &data[HEADER_SIZE..];
        let known = self.peers.contains_key(&sender);
        if !known {
            if self.peers.len() >= usize::from(MAX_PLAYERS - 1) {
                if self.slot.is_some() && now.duration_since(self.joined_at) < JOIN_TIME {
                    self.leave_full();
                }
                return;
            }
            self.peers.insert(
                sender,
                Peer {
                    slot,
                    last_seen: now,
                    settled: true,
                },
            );
            self.send_hello();
        }
        let theirs = {
            let peer = self.peers.get_mut(&sender).expect("peer inserted above");
            peer.slot = slot;
            peer.last_seen = now;
            if kind == HELLO {
                peer.settled = payload.first() == Some(&1);
            }
            peer.settled
        };
        if self.slot == Some(slot) {
            let mine = self.settled(now);
            if (theirs && !mine) || (theirs == mine && sender < self.instance_id) {
                if let Some(new_slot) = self.free_slot() {
                    self.slot = Some(new_slot);
                    self.send_hello();
                } else {
                    self.leave_full();
                    return;
                }
            }
        }
        if kind == DATA && self.inbox.len() < INBOX_LIMIT {
            self.inbox
                .push_back((slot, payload[..payload.len().min(MAX_PAYLOAD)].to_vec()));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn leave_full(&mut self) {
        self.full = true;
        self.slot = None;
        self.title = None;
        self.peers.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn make_instance_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32;
    let process = std::process::id().rotate_left(13);
    let id = NEXT_ID.fetch_add(0x9e37_79b9, Ordering::Relaxed) ^ time ^ process;
    if id == 0 { 1 } else { id }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(kind: u8, slot: u8, title: &[u8; 16], sender: u32, payload: &[u8]) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(b"SC16");
        packet.push(kind);
        packet.push(slot);
        packet.extend_from_slice(title);
        packet.extend_from_slice(&sender.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    #[test]
    fn packet_decoder_discards_wrong_magic_and_accepts_well_formed_header() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let mut title = [0; 16];
        title[..4].copy_from_slice(b"game");
        node.title = Some(title);
        node.slot = Some(0);
        node.instance_id = 9;
        let now = Instant::now();
        let mut malformed = packet(DATA, 1, &title, 10, b"hi");
        malformed[0] = b'X';
        node.handle_packet(&malformed, now);
        node.handle_packet(&packet(DATA, 1, &title, 10, b"hi"), now);
        assert_eq!(node.peers.len(), 1);
        assert_eq!(node.inbox, [(1, b"hi".to_vec())]);
    }

    #[test]
    fn high_network_mailbox_receives_bounded_packets_only_on_explicit_command() {
        use crate::machine::*;

        let mut node = NetNode::new(0);
        let title = *b"high-network-001";
        node.title = Some(title);
        node.slot = Some(0);
        node.instance_id = 9;
        node.handle_packet(&packet(DATA, 3, &title, 10, &[7; 100]), Instant::now());
        let mut device = NetworkDevice::default();
        for _ in 0..2 {
            assert_eq!(device.read(NETWORK_RECEIVE), Some(0));
            assert_eq!(device.read(NETWORK_RESULT), Some(0));
        }
        assert_eq!(node.inbox.len(), 1);
        device.command(0, &mut node).unwrap();
        assert_eq!(node.inbox.len(), 1);
        device.command(4, &mut node).unwrap();
        assert!(node.inbox.is_empty());
        assert_eq!(device.read(NETWORK_STATUS), Some(0));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(48));
        for offset in 0..48 {
            assert_eq!(device.read(NETWORK_RECEIVE + offset), Some(7));
        }
        device.write(NETWORK_QUERY, 4);
        device.command(5, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(3));
        node.inbox.push_back((2, b"x".to_vec()));
        device.command(4, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(1));
        assert_eq!(device.read(NETWORK_RECEIVE), Some(b'x'));
        for offset in 1..48 {
            assert_eq!(device.read(NETWORK_RECEIVE + offset), Some(0));
        }
        device.command(4, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(6));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(0));
        assert_eq!(device.read(NETWORK_RECEIVE), Some(b'x'));
        node.inbox.push_back((2, Vec::new()));
        device.command(4, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(0));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(0));
        assert_eq!(node.last_sender(), 2);
        for offset in 0..48 {
            assert_eq!(device.read(NETWORK_RECEIVE + offset), Some(0));
        }
    }

    #[test]
    fn high_network_send_reports_transport_failure_and_accepts_bounded_payloads() {
        use crate::machine::*;

        let mut node = NetNode::new(0);
        let mut device = NetworkDevice::default();
        node.title = Some(*b"high-network-001");
        node.slot = Some(0);
        device.write(NETWORK_LENGTH, 48);
        assert!(device.command(3, &mut node).is_err());
        assert_eq!(device.read(NETWORK_STATUS), Some(5));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(0));
        node.open(b"high-network-001").unwrap();
        node.port = node.socket.as_ref().unwrap().local_addr().unwrap().port();
        device.command(3, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(0));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(1));
        device.write(NETWORK_LENGTH, 0);
        device.command(3, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(0));
        device.write(NETWORK_LENGTH, 49);
        device.command(3, &mut node).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(2));
    }

    #[test]
    fn peer_slot_is_active_and_messages_are_bounded() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let mut title = [0; 16];
        title[..4].copy_from_slice(b"game");
        node.title = Some(title);
        node.slot = Some(0);
        node.instance_id = 9;
        let now = Instant::now();
        node.handle_packet(&packet(DATA, 3, &title, 10, &[7; 100]), now);
        assert!(node.active(0));
        assert!(node.active(3));
        assert!(!node.active(8));
        let (sender, message) = node.receive().unwrap();
        assert_eq!(sender, 3);
        assert_eq!(message.len(), MAX_PAYLOAD);
        assert_eq!(node.last_sender(), 3);
    }

    #[test]
    fn simulated_two_node_handshake_assigns_slots_and_routes_game_payloads() {
        let mut first = NetNode::new(DEFAULT_PORT);
        let mut second = NetNode::new(DEFAULT_PORT);
        let mut title = [0; 16];
        title[..4].copy_from_slice(b"game");
        let now = Instant::now();

        first.title = Some(title);
        first.slot = Some(0);
        first.instance_id = 1;
        first.joined_at = now - JOIN_TIME;
        second.title = Some(title);
        second.slot = Some(0);
        second.instance_id = 2;
        second.joined_at = now - JOIN_TIME;

        second.handle_packet(&packet(HELLO, 0, &title, 1, &[1]), now);
        assert_eq!(second.slot(), 1);
        first.handle_packet(&packet(HELLO, 1, &title, 2, &[1]), now);
        assert_eq!(first.slot(), 0);
        assert!(first.active(1));
        assert!(second.active(0));

        first.handle_packet(&packet(DATA, 1, &title, 2, b"player-two"), now);
        second.handle_packet(&packet(DATA, 0, &title, 1, b"player-one"), now);
        assert_eq!(first.receive(), Some((1, b"player-two".to_vec())));
        assert_eq!(second.receive(), Some((0, b"player-one".to_vec())));

        first.handle_packet(&packet(BYE, 1, &title, 2, b""), now);
        assert_eq!(first.players(), 1);
        assert!(!first.active(1));
    }

    #[test]
    fn udp_transport_delivers_handshake_data_and_departure() {
        let mut first = NetNode::new(0);
        let mut second = NetNode::new(0);
        let title = *b"udp-transport-01";
        first.open(&title).unwrap();
        second.open(&title).unwrap();
        first.instance_id = 1;
        second.instance_id = 2;
        let first_port = first.socket.as_ref().unwrap().local_addr().unwrap().port();
        let second_port = second.socket.as_ref().unwrap().local_addr().unwrap().port();
        let first_addr = (std::net::Ipv4Addr::LOCALHOST, first_port).into();
        let second_addr = (std::net::Ipv4Addr::LOCALHOST, second_port).into();

        second.send_packet_to(HELLO, &[0], first_addr);
        first.poll();
        first.send_packet_to(HELLO, &[0], second_addr);
        second.poll();
        assert_eq!(second.slot(), 1);
        second.send_packet_to(HELLO, &[0], first_addr);
        first.poll();
        assert!(first.active(1));

        second.send_packet_to(DATA, b"udp payload", first_addr);
        first.poll();
        assert_eq!(first.receive(), Some((1, b"udp payload".to_vec())));

        use crate::machine::*;
        second.send_packet_to(DATA, b"mailbox UDP", first_addr);
        let mut device = NetworkDevice::default();
        device.command(4, &mut first).unwrap();
        assert_eq!(device.read(NETWORK_STATUS), Some(0));
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(11));
        for (offset, byte) in b"mailbox UDP".iter().enumerate() {
            assert_eq!(device.read(NETWORK_RECEIVE + offset as u32), Some(*byte));
        }
        assert!(first.receive().is_none());
        device.write(NETWORK_QUERY, 4);
        device.command(5, &mut first).unwrap();
        assert_eq!(device.read(NETWORK_RESULT + 1), Some(1));

        second.send_packet_to(BYE, &[], first_addr);
        first.poll();
        assert_eq!(first.players(), 1);
        assert!(!first.active(1));
    }

    #[test]
    fn poll_removes_peers_after_timeout() {
        let mut node = NetNode::new(DEFAULT_PORT);
        node.title = Some(*b"timeout-test-000");
        node.slot = Some(0);
        node.peers.insert(
            10,
            Peer {
                slot: 1,
                last_seen: Instant::now() - PEER_TIMEOUT - Duration::from_millis(1),
                settled: true,
            },
        );

        node.poll();

        assert_eq!(node.players(), 1);
        assert!(!node.active(1));
    }

    #[test]
    fn established_full_group_ignores_additional_peers() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let title = *b"full-group-test!";
        let now = Instant::now();
        node.title = Some(title);
        node.slot = Some(0);
        node.joined_at = now - JOIN_TIME - Duration::from_millis(1);
        for slot in 1..MAX_PLAYERS {
            node.peers.insert(
                u32::from(slot) + 10,
                Peer {
                    slot,
                    last_seen: now,
                    settled: true,
                },
            );
        }

        node.handle_packet(&packet(DATA, 7, &title, 99, b"late peer"), now);

        assert_eq!(node.players(), u16::from(MAX_PLAYERS));
        assert!(!node.full);
        assert!(!node.peers.contains_key(&99));
        assert!(node.inbox.is_empty());
    }

    #[test]
    fn joining_node_leaves_when_group_is_full() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let title = *b"full-group-test!";
        let now = Instant::now();
        node.title = Some(title);
        node.slot = Some(0);
        node.joined_at = now;
        for slot in 1..MAX_PLAYERS {
            node.peers.insert(
                u32::from(slot) + 10,
                Peer {
                    slot,
                    last_seen: now,
                    settled: true,
                },
            );
        }

        node.handle_packet(&packet(HELLO, 7, &title, 99, &[0]), now);

        assert!(node.is_full());
        assert!(!node.is_open());
        assert_eq!(node.slot(), -1);
        assert!(node.peers.is_empty());
    }

    #[test]
    fn inbox_drops_messages_after_reaching_its_limit() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let title = *b"inbox-limit-0001";
        let now = Instant::now();
        node.title = Some(title);
        node.slot = Some(0);

        for index in 0..=INBOX_LIMIT {
            node.handle_packet(&packet(DATA, 1, &title, 10, &[index as u8]), now);
        }

        assert_eq!(node.inbox.len(), INBOX_LIMIT);
        for index in 0..INBOX_LIMIT {
            assert_eq!(node.receive(), Some((1, vec![index as u8])));
        }
        assert_eq!(node.receive(), None);
    }

    #[test]
    fn empty_payloads_are_valid_and_oversized_payloads_are_rejected() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let mut title = [0; 16];
        title[..4].copy_from_slice(b"game");
        node.title = Some(title);
        node.slot = Some(0);

        assert!(node.send(&[]));
        assert!(node.send(&[0; MAX_PAYLOAD]));
        assert!(!node.send(&[0; MAX_PAYLOAD + 1]));

        let now = Instant::now();
        node.handle_packet(&packet(DATA, 1, &title, 10, &[]), now);
        node.handle_packet(&packet(DATA, 1, &title, 10, &[7; MAX_PAYLOAD + 1]), now);
        assert_eq!(node.receive(), Some((1, Vec::new())));
        assert_eq!(node.receive(), Some((1, vec![7; MAX_PAYLOAD])));
    }

    #[test]
    fn network_packets_from_other_titles_and_invalid_slots_are_ignored() {
        let mut node = NetNode::new(DEFAULT_PORT);
        let mut title = [0; 16];
        title[..4].copy_from_slice(b"game");
        node.title = Some(title);
        node.slot = Some(0);
        let other_title = *b"another-game-000";
        let now = Instant::now();

        node.handle_packet(&packet(DATA, 1, &other_title, 10, b"wrong game"), now);
        node.handle_packet(&packet(DATA, MAX_PLAYERS, &title, 11, b"invalid slot"), now);
        node.handle_packet(&packet(BYE, 1, &title, 12, b""), now);
        assert!(node.peers.is_empty());
        assert!(node.inbox.is_empty());
    }
}
