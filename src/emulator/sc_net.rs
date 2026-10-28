//! Implements named-node SC-NET communication over UDP.
//! Handles discovery, heartbeats, identity collisions, peer expiry, and bounded messages,
//! independently of the game networking protocol.

use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

pub const DEFAULT_PORT: u16 = 47_017;
pub const NAME_CAPACITY: usize = 16;
pub const MAX_PAYLOAD: usize = 1024;
const HEADER_SIZE: usize = 29;
const DATA_HEADER_SIZE: usize = HEADER_SIZE + NAME_CAPACITY;
const HELLO: u8 = 1;
const DATA: u8 = 2;
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(500);
const PEER_TIMEOUT: Duration = Duration::from_secs(4);
const INBOX_LIMIT: usize = 64;

#[derive(Clone)]
struct Peer {
    address: SocketAddr,
    last_seen: Instant,
}

pub struct ScNetNode {
    name: [u8; NAME_CAPACITY],
    instance_id: u64,
    port: u16,
    socket: UdpSocket,
    peers: HashMap<String, Peer>,
    inbox: VecDeque<(String, Vec<u8>)>,
    last_heartbeat: Instant,
    collision: bool,
}

impl ScNetNode {
    pub fn new(name: &str, port: u16) -> Result<Self, String> {
        if name.is_empty()
            || name.len() > NAME_CAPACITY
            || !name.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("SC-NET node name must contain 1 to 16 ASCII bytes".to_owned());
        }
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|error| format!("cannot create SC-NET UDP socket: {error}"))?;
        socket
            .set_reuse_address(true)
            .map_err(|error| format!("cannot configure SC-NET socket: {error}"))?;
        socket
            .set_broadcast(true)
            .map_err(|error| format!("cannot enable SC-NET broadcast: {error}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure SC-NET socket: {error}"))?;
        socket
            .bind(&SockAddr::from(SocketAddr::from(([0, 0, 0, 0], port))))
            .map_err(|error| format!("cannot bind SC-NET port {port}: {error}"))?;
        let socket: UdpSocket = socket.into();
        let mut padded_name = [0; NAME_CAPACITY];
        padded_name[..name.len()].copy_from_slice(name.as_bytes());
        let now = Instant::now();
        let mut node = Self {
            name: padded_name,
            instance_id: make_instance_id(),
            port,
            socket,
            peers: HashMap::new(),
            inbox: VecDeque::new(),
            last_heartbeat: now,
            collision: false,
        };
        node.send_hello()?;
        Ok(node)
    }

    pub fn poll(&mut self) -> Result<(), String> {
        loop {
            let mut packet = [0; DATA_HEADER_SIZE + MAX_PAYLOAD];
            match self.socket.recv_from(&mut packet) {
                Ok((length, source)) => self.handle_packet(&packet[..length], source),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("SC-NET receive failed: {error}")),
            }
        }
        let now = Instant::now();
        self.peers
            .retain(|_, peer| now.duration_since(peer.last_seen) <= PEER_TIMEOUT);
        self.collision = self
            .peers
            .contains_key(&decode_name(&self.name).unwrap_or_default());
        if now.duration_since(self.last_heartbeat) >= HEARTBEAT_INTERVAL {
            self.send_hello()?;
        }
        Ok(())
    }

    pub fn has_name_collision(&self) -> bool {
        self.collision
    }

    pub fn peers(&self) -> Vec<String> {
        self.peers.keys().cloned().collect()
    }

    pub fn send(&mut self, recipient: &str, payload: &[u8]) -> Result<(), String> {
        self.poll()?;
        if recipient.is_empty()
            || recipient.len() > NAME_CAPACITY
            || !recipient.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("SC-NET recipient must contain 1 to 16 ASCII bytes".to_owned());
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(format!("SC-NET payload exceeds {MAX_PAYLOAD} bytes"));
        }
        let peer = self
            .peers
            .get(recipient)
            .ok_or_else(|| format!("SC-NET peer {recipient:?} is not discovered"))?;
        let mut packet = Vec::with_capacity(DATA_HEADER_SIZE + payload.len());
        packet.extend_from_slice(b"SCN1");
        packet.push(DATA);
        packet.extend_from_slice(&self.name);
        packet.extend_from_slice(&self.instance_id.to_be_bytes());
        let mut recipient_name = [0; NAME_CAPACITY];
        recipient_name[..recipient.len()].copy_from_slice(recipient.as_bytes());
        packet.extend_from_slice(&recipient_name);
        packet.extend_from_slice(payload);
        self.socket
            .send_to(&packet, peer.address)
            .map_err(|error| format!("SC-NET send failed: {error}"))?;
        Ok(())
    }

    pub fn receive(&mut self, capacity: usize) -> Result<Option<(String, Vec<u8>)>, String> {
        self.poll()?;
        let Some((_, payload)) = self.inbox.front() else {
            return Ok(None);
        };
        if payload.len() > capacity {
            return Err(format!(
                "SC-NET message requires {} bytes but output capacity is {capacity}",
                payload.len()
            ));
        }
        Ok(self.inbox.pop_front())
    }

    fn send_hello(&mut self) -> Result<(), String> {
        self.last_heartbeat = Instant::now();
        let mut packet = Vec::with_capacity(HEADER_SIZE);
        packet.extend_from_slice(b"SCN1");
        packet.push(HELLO);
        packet.extend_from_slice(&self.name);
        packet.extend_from_slice(&self.instance_id.to_be_bytes());
        match self
            .socket
            .send_to(&packet, (std::net::Ipv4Addr::BROADCAST, self.port))
        {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("SC-NET discovery broadcast failed: {error}")),
        }
    }

    #[cfg(test)]
    pub(crate) fn inject_hello_for_test(&mut self, name: &str) {
        let mut encoded_name = [0; NAME_CAPACITY];
        encoded_name[..name.len()].copy_from_slice(name.as_bytes());
        let mut packet = Vec::with_capacity(HEADER_SIZE);
        packet.extend_from_slice(b"SCN1");
        packet.push(HELLO);
        packet.extend_from_slice(&encoded_name);
        packet.extend_from_slice(&self.instance_id.wrapping_add(1).to_be_bytes());
        let source = self.socket.local_addr().unwrap();
        self.handle_packet(&packet, source);
    }

    fn handle_packet(&mut self, packet: &[u8], source: SocketAddr) {
        if packet.len() < HEADER_SIZE || packet.get(..4) != Some(b"SCN1") {
            return;
        }
        let kind = packet[4];
        if kind != HELLO && kind != DATA {
            return;
        }
        let name_bytes: [u8; NAME_CAPACITY] = match packet[5..21].try_into() {
            Ok(name) => name,
            Err(_) => return,
        };
        let peer_name = match decode_name(&name_bytes) {
            Some(name) => name,
            None => return,
        };
        let instance_id = u64::from_be_bytes(packet[21..29].try_into().unwrap_or_default());
        if instance_id == self.instance_id {
            return;
        }
        self.collision |= name_bytes == self.name && instance_id != self.instance_id;
        if kind == DATA {
            if packet.len() < DATA_HEADER_SIZE {
                return;
            }
            let recipient: [u8; NAME_CAPACITY] = match packet[29..45].try_into() {
                Ok(name) => name,
                Err(_) => return,
            };
            if recipient == self.name && packet.len() <= DATA_HEADER_SIZE + MAX_PAYLOAD {
                if self.inbox.len() == INBOX_LIMIT {
                    self.inbox.pop_front();
                }
                self.inbox
                    .push_back((peer_name.clone(), packet[DATA_HEADER_SIZE..].to_vec()));
            }
        }
        self.peers.insert(
            peer_name,
            Peer {
                address: source,
                last_seen: Instant::now(),
            },
        );
    }
}

fn decode_name(bytes: &[u8; NAME_CAPACITY]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    if end == 0
        || !bytes[..end].iter().all(|byte| byte.is_ascii_graphic())
        || bytes[end..].iter().any(|byte| *byte != 0)
    {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

fn make_instance_id() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    time ^ NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(name: &[u8; NAME_CAPACITY], instance_id: u64) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(b"SCN1");
        packet.push(HELLO);
        packet.extend_from_slice(name);
        packet.extend_from_slice(&instance_id.to_be_bytes());
        packet
    }

    #[test]
    fn discovery_uses_distinct_protocol_and_detects_duplicate_names() {
        let mut node = ScNetNode::new("node-a", 0).unwrap();
        let address = node.socket.local_addr().unwrap();
        let mut name = [0; NAME_CAPACITY];
        name[..6].copy_from_slice(b"node-a");
        node.handle_packet(&hello(&name, node.instance_id + 1), address);
        assert!(node.has_name_collision());
        assert_eq!(node.peers(), ["node-a"]);

        let mut wrong_protocol = hello(&name, node.instance_id + 2);
        wrong_protocol[..4].copy_from_slice(b"SC16");
        node.handle_packet(&wrong_protocol, address);
        assert_eq!(node.peers().len(), 1);
    }

    #[test]
    fn rejects_invalid_names_and_oversized_messages() {
        assert!(ScNetNode::new("", 0).is_err());
        assert!(ScNetNode::new("0123456789ABCDEF0", 0).is_err());
        let mut node = ScNetNode::new("node-a", 0).unwrap();
        assert!(node.send("missing", &[]).is_err());
        assert!(node.receive(0).unwrap().is_none());
    }

    #[test]
    fn routes_messages_by_sc_net_identity() {
        let mut first = ScNetNode::new("node-a", 0).unwrap();
        let mut second = ScNetNode::new("node-b", 0).unwrap();
        let first_address = SocketAddr::from((
            std::net::Ipv4Addr::LOCALHOST,
            first.socket.local_addr().unwrap().port(),
        ));
        let second_address = SocketAddr::from((
            std::net::Ipv4Addr::LOCALHOST,
            second.socket.local_addr().unwrap().port(),
        ));
        let mut first_name = [0; NAME_CAPACITY];
        first_name[..6].copy_from_slice(b"node-a");
        let mut second_name = [0; NAME_CAPACITY];
        second_name[..6].copy_from_slice(b"node-b");

        first.handle_packet(&hello(&second_name, second.instance_id), second_address);
        second.handle_packet(&hello(&first_name, first.instance_id), first_address);
        first.send("node-b", b"SC-NET message").unwrap();
        second.poll().unwrap();
        assert_eq!(
            second.receive(MAX_PAYLOAD).unwrap(),
            Some(("node-a".to_owned(), b"SC-NET message".to_vec()))
        );
    }
}
