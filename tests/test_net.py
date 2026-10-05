import socket
import time
import unittest

from gfx_harness import ROOT, compile_file, make_cpu, pixel, run_frame
from tools.sc16net import JOIN_TIME, MAX_PLAYERS, PEER_TIMEOUT, NetNode, UdpTransport


class Hub:
    """In-memory broadcast domain; every endpoint hears everyone else."""

    def __init__(self):
        self.endpoints = []

    def endpoint(self):
        end = Endpoint(self)
        self.endpoints.append(end)
        return end


class Endpoint:
    def __init__(self, hub):
        self.hub = hub
        self.queue = []

    def send(self, data):
        for end in self.hub.endpoints:
            if end is not self:
                end.queue.append(data)

    def receive(self):
        return self.queue.pop(0) if self.queue else None

    def close(self):
        pass


class Clock:
    now = 100.0

    def __call__(self):
        return self.now


def settle(nodes, clock, seconds=1.0, step=0.1):
    for _ in range(int(seconds / step)):
        clock.now += step
        for node in nodes:
            node.poll()


def make_nodes(count, title=b"game", clock=None, hub=None):
    clock = clock or Clock()
    hub = hub or Hub()
    nodes = [NetNode(hub.endpoint(), clock, instance_id=i + 1) for i in range(count)]
    for node in nodes:
        node.open(title)
    return nodes, clock, hub


class NetNodeTests(unittest.TestCase):
    def test_peers_get_unique_slots_and_see_each_other(self):
        nodes, clock, _ = make_nodes(4)
        settle(nodes, clock, 2.0)
        self.assertEqual(sorted(n.slot for n in nodes), [0, 1, 2, 3])
        for node in nodes:
            self.assertEqual(node.players(), 4)
            self.assertTrue(node.ready)
            self.assertTrue(all(node.active(s) for s in range(4)))
            self.assertFalse(node.active(4))

    def test_broadcast_reaches_everyone_but_sender(self):
        nodes, clock, _ = make_nodes(3)
        settle(nodes, clock)
        self.assertTrue(nodes[0].send(b"hello"))
        self.assertIsNone(nodes[0].recv())
        for node in nodes[1:]:
            slot, payload = node.recv()
            self.assertEqual((slot, payload), (nodes[0].slot, b"hello"))
            self.assertIsNone(node.recv())

    def test_other_titles_are_ignored(self):
        hub = Hub()
        clock = Clock()
        pacman, _, _ = make_nodes(2, b"pacman", clock, hub)
        elite, _, _ = make_nodes(2, b"elite", clock, hub)
        settle(pacman + elite, clock, 2.0)
        self.assertEqual(pacman[0].players(), 2)
        self.assertEqual(elite[0].players(), 2)
        pacman[0].send(b"x")
        self.assertIsNotNone(pacman[1].recv())
        self.assertIsNone(elite[0].recv())
        self.assertIsNone(elite[1].recv())

    def test_group_is_limited_to_eight_players(self):
        nodes, clock, hub = make_nodes(MAX_PLAYERS)
        settle(nodes, clock, 2.0)
        self.assertEqual(sorted(n.slot for n in nodes), list(range(MAX_PLAYERS)))
        late = NetNode(hub.endpoint(), clock, instance_id=99)
        late.open(b"game")
        settle(nodes + [late], clock, 2.0)
        self.assertTrue(late.full)
        self.assertFalse(late.is_open)
        self.assertEqual(nodes[0].players(), MAX_PLAYERS)

    def test_leaving_and_timeouts_free_slots(self):
        nodes, clock, hub = make_nodes(3)
        settle(nodes, clock, 2.0)
        nodes[2].close()
        settle(nodes[:2], clock, 0.3)
        self.assertEqual(nodes[0].players(), 2)
        nodes[1].transport.queue.clear()
        # A crashed peer stops sending and times out
        settle([nodes[0]], clock, PEER_TIMEOUT + 1.0)
        self.assertEqual(nodes[0].players(), 1)

    def test_rejects_oversized_and_unjoined_sends(self):
        nodes, clock, _ = make_nodes(1)
        self.assertFalse(nodes[0].send(b"x" * 200))
        nodes[0].close()
        self.assertFalse(nodes[0].send(b"x"))
        self.assertFalse(nodes[0].open(b""))


class UdpTests(unittest.TestCase):
    def test_real_udp_sockets_exchange_messages(self):
        ports = []
        for _ in range(2):
            probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            probe.bind(("127.0.0.1", 0))
            ports.append(probe.getsockname()[1])
            probe.close()
        targets = [("127.0.0.1", p) for p in ports]
        nodes = [NetNode(UdpTransport(p, targets)) for p in ports]
        try:
            for node in nodes:
                node.open(b"udp-test")
            deadline = time.monotonic() + 3 + JOIN_TIME
            while time.monotonic() < deadline and not all(n.ready and n.players() == 2 for n in nodes):
                for node in nodes:
                    node.poll()
                time.sleep(0.02)
            self.assertEqual([n.players() for n in nodes], [2, 2])
            self.assertNotEqual(nodes[0].slot, nodes[1].slot)
            nodes[0].send(b"ping")
            message = None
            deadline = time.monotonic() + 2
            while message is None and time.monotonic() < deadline:
                message = nodes[1].recv()
                time.sleep(0.02)
            self.assertEqual(message, (nodes[0].slot, b"ping"))
        finally:
            for node in nodes:
                node.close()
                node.transport.close()


class NetGameDemoTests(unittest.TestCase):
    def test_demo_instances_see_each_other_and_ping(self):
        hub, clock = Hub(), Clock()
        image = compile_file(ROOT / "examples" / "net_game_demo.c")
        cpus = []
        for i in range(2):
            cpu = make_cpu(image)
            cpu.net = NetNode(hub.endpoint(), clock, instance_id=i + 1)
            cpus.append(cpu)
        def frames(count, keys0=0):
            for _ in range(count):
                clock.now += 0.1
                cpus[0].memory[0x43004] = keys0
                for cpu in cpus:
                    run_frame(cpu)

        def count(cpu, color):
            return sum(1 for y in range(240) for x in range(320) if pixel(cpu, x, y) == color)

        frames(30)
        self.assertEqual([cpu.net.players() for cpu in cpus], [2, 2])
        self.assertEqual(sorted(cpu.net.slot for cpu in cpus), [0, 1])
        # Move instance 0 right (KEY_RIGHT = 2); instance 1 must see it in its slot colour
        frames(20, 2)
        color = [2, 3][cpus[0].net.slot]
        frames(2)
        self.assertEqual(pixel(cpus[1], 192, 124), color)
        before = count(cpus[1], color)
        frames(1, 16)  # KEY_FIRE: broadcast a ping ring
        frames(3)
        self.assertGreater(count(cpus[1], color), before)


if __name__ == "__main__":
    unittest.main()
