"""Serverless LAN networking for SC-16 games.

Every emulator instance is a peer.  Peers find each other by broadcasting small
UDP datagrams on the local network, so no dedicated server program is needed.

Packet layout (big endian):
    4s magic "SC16" | B type | B slot | 16s game title | I instance id | payload

Only instances that use the same game title see each other, so "pacman"
ignores "elite".  At most MAX_PLAYERS instances share one title; each owns a
stable slot 0..7.
"""
import random
import socket
import struct
import time

DEFAULT_PORT = 47016
MAX_PLAYERS = 8
MAX_PAYLOAD = 48
TITLE_LEN = 16
HELLO_INTERVAL = 1.0
JOIN_INTERVAL = 0.2
JOIN_TIME = 0.6
PEER_TIMEOUT = 3.5
INBOX_LIMIT = 64

MAGIC = b"SC16"
HEADER = struct.Struct(">4sBB16sI")
HELLO, DATA, BYE = 1, 2, 3


class UdpTransport:
    """Non-blocking UDP broadcast socket shared by all instances on a port."""

    def __init__(self, port=DEFAULT_PORT, targets=None):
        self.targets = targets or [("255.255.255.255", port)]
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        if hasattr(socket, "SO_REUSEPORT"):
            self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        self.sock.bind(("", port))
        self.sock.setblocking(False)

    def send(self, data):
        for target in self.targets:
            try:
                self.sock.sendto(data, target)
            except OSError:
                pass

    def receive(self):
        try:
            return self.sock.recvfrom(1024)[0]
        except (BlockingIOError, InterruptedError, ConnectionResetError):
            return None
        except OSError:
            return None

    def close(self):
        self.sock.close()


class NetNode:
    """One game instance on the network."""

    def __init__(self, transport, clock=time.monotonic, instance_id=None):
        self.transport = transport
        self.clock = clock
        self.instance_id = instance_id or random.getrandbits(32) or 1
        self.title = None
        self.slot = None
        self.full = False
        self.joined_at = 0.0
        self.last_hello = 0.0
        self.peers = {}          # instance id -> [slot, last seen, settled]
        self.inbox = []
        self.last_sender = 0

    @property
    def is_open(self):
        return self.title is not None

    @property
    def ready(self):
        return self.is_open and self.slot is not None and self.clock() - self.joined_at >= JOIN_TIME

    def open(self, title):
        """Join the game group for title; returns False when the title is empty."""
        self.close()
        self.title = bytes(title)[:TITLE_LEN].ljust(TITLE_LEN, b"\0")
        if not self.title.strip(b"\0"):
            self.title = None
            return False
        self.peers.clear()
        self.inbox.clear()
        self.full = False
        self.slot = 0
        self.joined_at = self.last_hello = self.clock()
        self._hello()
        return True

    def close(self):
        if self.is_open and self.slot is not None:
            self._send(BYE)
        self.title = None
        self.slot = None
        self.peers.clear()
        self.inbox.clear()

    def players(self):
        """Number of live instances in the group, including this one."""
        return len(self.peers) + 1 if self.slot is not None else 0

    def active(self, slot):
        if self.slot is None or not 0 <= slot < MAX_PLAYERS:
            return False
        return slot == self.slot or any(p[0] == slot for p in self.peers.values())

    def send(self, payload):
        """Broadcast payload to every other instance with the same title."""
        if self.slot is None or len(payload) > MAX_PAYLOAD:
            return False
        self._send(DATA, bytes(payload))
        return True

    def recv(self):
        """Pop the oldest message as (sender slot, payload), or None."""
        self.poll()
        if not self.inbox:
            return None
        slot, payload = self.inbox.pop(0)
        self.last_sender = slot
        return slot, payload

    def poll(self):
        """Process incoming packets, expire dead peers and send keep-alives."""
        if not self.is_open:
            return
        while True:
            data = self.transport.receive()
            if data is None:
                break
            self._handle(data)
        now = self.clock()
        for key in [k for k, p in self.peers.items() if now - p[1] > PEER_TIMEOUT]:
            del self.peers[key]
        if self.slot is not None:
            joining = now - self.joined_at < JOIN_TIME
            if now - self.last_hello >= (JOIN_INTERVAL if joining else HELLO_INTERVAL):
                self._hello()

    def _free_slot(self):
        taken = {p[0] for p in self.peers.values()}
        for slot in range(MAX_PLAYERS):
            if slot not in taken:
                return slot
        return None

    def _send(self, kind, payload=b""):
        slot = self.slot if self.slot is not None else 0
        self.transport.send(HEADER.pack(MAGIC, kind, slot, self.title, self.instance_id) + payload)

    @property
    def settled(self):
        return self.slot is not None and self.clock() - self.joined_at >= JOIN_TIME

    def _hello(self):
        self.last_hello = self.clock()
        self._send(HELLO, b"\x01" if self.settled else b"\x00")

    def _leave_full(self):
        self.full = True
        self.slot = None
        self.title = None
        self.peers.clear()

    def _handle(self, data):
        if len(data) < HEADER.size:
            return
        magic, kind, slot, title, sender = HEADER.unpack_from(data)
        if magic != MAGIC or title != self.title or sender == self.instance_id or slot >= MAX_PLAYERS:
            return
        payload = data[HEADER.size:]
        now = self.clock()
        if kind == BYE:
            self.peers.pop(sender, None)
            return
        known = sender in self.peers
        if not known:
            if len(self.peers) >= MAX_PLAYERS - 1:
                if self.slot is not None and self.clock() - self.joined_at < JOIN_TIME:
                    self._leave_full()
                return
            self.peers[sender] = [slot, now, True]
            self._hello()           # introduce ourselves to the newcomer
        entry = self.peers[sender]
        entry[0], entry[1] = slot, now
        if kind == HELLO:
            entry[2] = payload[:1] == b"\x01"
        if self.slot is not None and slot == self.slot:
            # Slot clash: a settled player keeps its slot against a newcomer;
            # otherwise the lower instance id wins.
            mine = self.settled
            theirs = entry[2]
            if (theirs and not mine) or (theirs == mine and sender < self.instance_id):
                new_slot = self._free_slot()
                if new_slot is None:
                    self._leave_full()
                    return
                self.slot = new_slot
                self._hello()
        if kind == DATA and len(self.inbox) < INBOX_LIMIT:
            self.inbox.append((slot, payload[:MAX_PAYLOAD]))
