import unittest

import c_compiler
from c_compiler import GLOBAL_BASE
from gfx_harness import ROOT, compile_file, make_cpu, pixel, run_frame
from test_net import Clock, Hub
from tools.sc16net import NetNode

SOURCE = ROOT / "modules" / "spacecave.c"


def global_offsets():
    text = SOURCE.read_text(encoding="utf-8")
    parser = c_compiler._Parser(text, str(SOURCE))
    globals_, functions = parser.parse()
    generator = c_compiler._CodeGenerator(globals_, functions, str(SOURCE))
    generator.prototypes = parser.prototypes
    generator.generate()
    offsets = {name: address - GLOBAL_BASE for name, address in generator.global_addresses.items()}
    size = sum(generator._global_size(item) for item in globals_)
    return offsets, size


OFFSETS, GLOBALS_SIZE = global_offsets()
IMAGE = compile_file(SOURCE)
KEYS = 0x43004
CHAR = 0x43000


class Game:
    """Several SpaceCave instances connected through an in-memory LAN."""

    def __init__(self, count):
        self.hub, self.clock = Hub(), Clock()
        self.cpus = []
        for i in range(count):
            cpu = make_cpu(IMAGE)
            cpu.net = NetNode(self.hub.endpoint(), self.clock, instance_id=i + 1)
            run_frame(cpu, 50_000_000)  # menu cave generation takes several frames
            self.cpus.append(cpu)
        self.keys = [0] * count

    def frames(self, count):
        for _ in range(count):
            self.clock.now += 1 / 60
            for cpu, keys in zip(self.cpus, self.keys):
                cpu.memory[KEYS] = keys
                run_frame(cpu)

    def type_name(self, index, name):
        cpu = self.cpus[index]
        for ch in name:
            cpu.memory[CHAR] = ord(ch)
            run_frame(cpu)
        cpu.memory[KEYS] = 32
        run_frame(cpu)
        cpu.memory[KEYS] = 0
        run_frame(cpu)

    def join(self, index, name):
        self.type_name(index, name)

    def var(self, index, name, item=0, size=2, signed=True):
        address = GLOBAL_BASE + OFFSETS[name] + item * size
        cpu = self.cpus[index]
        if size == 1:
            return cpu.memory[address]
        value = cpu.read_16(address)
        return value - 65536 if signed and value > 32767 else value

    def names(self, index):
        base = GLOBAL_BASE + OFFSETS["names"]
        memory = self.cpus[index].memory
        return [bytes(memory[base + t * 9:base + t * 9 + 8]).decode().strip() for t in range(8)]


def start(count, name_prefix="P", settle=120, mission=False):
    game = Game(count)
    for i in range(count):
        if mission:
            game.cpus[i].memory[KEYS] = 1  # LEFT toggles the mode in the menu
            run_frame(game.cpus[i])
            game.cpus[i].memory[KEYS] = 0
            run_frame(game.cpus[i])
        game.join(i, f"{name_prefix}{i}")
        game.frames(3)
    game.frames(settle)
    return game


def host_of(game):
    return min(range(len(game.cpus)), key=lambda i: game.cpus[i].net.slot)


def put(game, index, name, value, entry=0, size=2):
    address = GLOBAL_BASE + OFFSETS[name] + entry * size
    if size == 1:
        game.cpus[index].memory[address] = value
    else:
        game.cpus[index].write_16(address, value & 0xFFFF)


def force_objective(game, index, otype, items):
    put(game, index, "otype", otype)
    put(game, index, "ob_left", len(items))
    put(game, index, "ob_hp", 100)
    for k in range(6):
        x, y = items[k] if k < len(items) else (-1, 0)
        put(game, index, "obx", x, k)
        put(game, index, "oby", y, k)


def park(game, index, entry, x, y):
    for name, value in (("px", x), ("py", y), ("pvx", 0), ("pvy", 0)):
        put(game, index, name, value, entry)
    put(game, index, "pinv", 0, entry, 1)
    cave = GLOBAL_BASE + OFFSETS["cave"]
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            game.cpus[index].memory[cave + ((y >> 4) + dy) * 64 + (x >> 4) + dx] = 0


class MissionTests(unittest.TestCase):
    def test_classic_mode_has_no_drones_or_target(self):
        game = start(1)
        self.assertEqual(game.var(0, "gmode"), 0)
        self.assertEqual([game.var(0, "dhp", k, 1) for k in range(4)], [0] * 4)
        self.assertEqual(game.var(0, "obx", 0), -1)

    def test_mission_mode_spawns_target_and_drones_and_syncs_clients(self):
        game = start(2, mission=True)
        for i in range(2):
            self.assertEqual(game.var(i, "gmode"), 1)
            self.assertGreaterEqual(game.var(i, "obx", 0), 0)
            self.assertEqual([game.var(i, "dhp", k, 1) for k in range(4)], [20] * 4)
        self.assertEqual(game.var(0, "otype"), game.var(1, "otype"))
        self.assertEqual(game.var(0, "otype"), game.var(0, "cave_seed", size=2, signed=False) % 3)
        self.assertEqual(game.var(0, "obx", 0), game.var(1, "obx", 0))

    def test_client_adopts_hosts_mode(self):
        game = Game(2)
        game.cpus[0].memory[KEYS] = 1  # LEFT toggles Mission mode in the menu
        run_frame(game.cpus[0])
        game.cpus[0].memory[KEYS] = 0
        run_frame(game.cpus[0])
        for i in (0, 1):
            game.join(i, f"P{i}")
            game.frames(3)
        game.frames(120)
        self.assertEqual(game.var(0, "gmode"), 1)
        self.assertEqual(game.var(1, "gmode"), 1)

    def test_drones_roam(self):
        game = start(1, mission=True)
        before = [(game.var(0, "dfx", k), game.var(0, "dfy", k)) for k in range(4)]
        game.frames(120)
        after = [(game.var(0, "dfx", k), game.var(0, "dfy", k)) for k in range(4)]
        self.assertNotEqual(before, after)

    def test_beacon_gives_bonus_and_moves(self):
        game = start(2, mission=True)
        host = host_of(game)
        me = game.var(host, "me")
        x, y = game.var(host, "obx", 0), game.var(host, "oby", 0)
        force_objective(game, host, 0, [(x, y)])
        put(game, host, "pscore", 0, me)
        park(game, host, me, x, y)
        game.frames(2)
        self.assertGreaterEqual(game.var(host, "pscore", me), 150)
        moved = (game.var(host, "obx", 0), game.var(host, "oby", 0))
        self.assertNotEqual(moved, (x, y))
        game.frames(10)
        self.assertEqual(game.var(1 - host, "pscore", me), game.var(host, "pscore", me))

    def test_reactor_destroyed_by_shots(self):
        game = start(1, mission=True)
        me = game.var(0, "me")
        x, y = game.var(0, "obx", 0), game.var(0, "oby", 0)
        force_objective(game, 0, 1, [(x, y)])
        put(game, 0, "ob_hp", 20)
        put(game, 0, "pscore", 0, me)
        park(game, 0, me, x - 60, y)
        put(game, 0, "blife", 5, 0, 1)
        put(game, 0, "bx", x - 2, 0)
        put(game, 0, "by", y, 0)
        put(game, 0, "bvx", 0, 0)
        put(game, 0, "bvy", 0, 0)
        put(game, 0, "bown", me, 0, 1)
        game.frames(2)
        self.assertEqual(game.var(0, "pscore", me), 300)
        self.assertEqual(game.var(0, "obx", 0), -1)
        game.frames(620)
        self.assertGreaterEqual(game.var(0, "obx", 0), 0)
        self.assertEqual(game.var(0, "ob_hp"), 100)

    def test_crystals_score_each_and_set_bonus(self):
        game = start(1, mission=True)
        me = game.var(0, "me")
        a = (game.var(0, "chx", 0, 1) * 16 + 8, game.var(0, "chy", 0, 1) * 16 + 8)
        b = (game.var(0, "chx", 1, 1) * 16 + 8, game.var(0, "chy", 1, 1) * 16 + 8)
        force_objective(game, 0, 2, [a, b])
        put(game, 0, "pscore", 0, me)
        park(game, 0, me, *a)
        game.frames(2)
        self.assertEqual(game.var(0, "pscore", me), 40)
        self.assertEqual(game.var(0, "ob_left"), 1)
        park(game, 0, me, *b)
        game.frames(2)
        self.assertEqual(game.var(0, "pscore", me), 180)
        self.assertEqual(game.var(0, "ob_left"), 6)  # a fresh set of six

    def test_shooting_a_drone_scores(self):
        game = start(1, mission=True)
        me = game.var(0, "me")
        park(game, 0, me, 100, 100)
        x, y = game.var(0, "chx", 5, 1) * 16 + 8, game.var(0, "chy", 5, 1) * 16 + 8
        put(game, 0, "dfx", x << 3, 0)
        put(game, 0, "dfy", y << 3, 0)
        put(game, 0, "dhp", 20, 0, 1)
        put(game, 0, "pscore", 0, me)
        put(game, 0, "blife", 5, 0, 1)
        put(game, 0, "bx", x, 0)
        put(game, 0, "by", y, 0)
        put(game, 0, "bvx", 0, 0)
        put(game, 0, "bvy", 0, 0)
        put(game, 0, "bown", me, 0, 1)
        game.frames(1)
        self.assertEqual(game.var(0, "dhp", 0, 1), 0)
        self.assertEqual(game.var(0, "pscore", me), 25)

    def test_drone_rams_pilot(self):
        game = start(1, mission=True)
        me = game.var(0, "me")
        x, y = game.var(0, "chx", 5, 1) * 16 + 8, game.var(0, "chy", 5, 1) * 16 + 8
        park(game, 0, me, x, y)
        put(game, 0, "pinv", 0, me, 1)
        put(game, 0, "dfx", x << 3, 1)
        put(game, 0, "dfy", y << 3, 1)
        put(game, 0, "dhp", 20, 1, 1)
        game.frames(1)
        self.assertEqual(game.var(0, "dhp", 1, 1), 0)
        self.assertLess(game.var(0, "php", me, 1), 100)
        self.assertGreater(game.var(0, "php", me, 1), 0)


class SpaceCaveTests(unittest.TestCase):
    def test_fits_memory_budget(self):
        self.assertLess(GLOBAL_BASE + GLOBALS_SIZE + 700, 0x10000)  # room for strings

    def test_menu_name_entry_and_backspace(self):
        game = Game(1)
        cpu = game.cpus[0]
        for ch in "abcX":
            cpu.memory[CHAR] = ord(ch)
            run_frame(cpu)
        cpu.memory[CHAR] = 8
        run_frame(cpu)
        self.assertEqual(game.var(0, "name_len"), 3)
        base = GLOBAL_BASE + OFFSETS["tname"]
        self.assertEqual(bytes(cpu.memory[base:base + 3]), b"ABC")
        self.assertEqual(game.var(0, "state"), 0)

    def test_menu_controls_page_toggles_with_tab(self):
        game = Game(1)
        game.cpus[0].memory[KEYS] = 64
        run_frame(game.cpus[0])
        game.cpus[0].memory[KEYS] = 0
        run_frame(game.cpus[0])
        self.assertEqual(game.var(0, "show_help"), 1)

    def test_enter_without_name_does_not_join(self):
        game = Game(1)
        game.cpus[0].memory[KEYS] = 32
        run_frame(game.cpus[0])
        self.assertEqual(game.var(0, "state"), 0)

    def test_two_players_share_identical_cave_and_distinct_colours(self):
        game = start(2)
        base = GLOBAL_BASE + OFFSETS["cave"]
        caves = [bytes(cpu.memory[base:base + 3072]) for cpu in game.cpus]
        self.assertEqual(caves[0], caves[1])
        self.assertGreater(sum(1 for c in caves[0] if c == 0), 400)
        self.assertEqual(game.names(0)[:2], ["P0", "P1"])
        self.assertEqual(game.names(0)[:2], game.names(1)[:2])
        used = [game.var(0, "pused", t, 1) for t in range(8)]
        self.assertEqual(used[:2], [1, 1])
        self.assertEqual(sum(1 for u in used if u == 1), 2)
        host = min(range(2), key=lambda i: game.cpus[i].net.slot)
        self.assertEqual(game.var(host, "i_am_host"), 1)
        self.assertEqual(game.var(1 - host, "i_am_host"), 0)

    def test_spawns_are_safe(self):
        game = start(4)
        cave = GLOBAL_BASE + OFFSETS["cave"]
        for t in range(4):
            x, y = game.var(0, "px", t), game.var(0, "py", t)
            tile = (y >> 4) * 64 + (x >> 4)
            self.assertEqual(game.cpus[0].memory[cave + tile], 0)
            for u in range(t):
                distance = abs(x - game.var(0, "px", u)) + abs(y - game.var(0, "py", u))
                self.assertGreaterEqual(distance, 60)

    def test_client_ship_follows_snapshots_and_ping_is_measured(self):
        game = start(2)
        host = min(range(2), key=lambda i: game.cpus[i].net.slot)
        client = 1 - host
        me = game.var(client, "me")
        before = game.var(host, "px", me), game.var(host, "py", me)
        game.keys[client] = 4  # thrust
        game.frames(60)
        game.keys[client] = 0
        game.frames(6)
        after = game.var(host, "px", me), game.var(host, "py", me)
        self.assertNotEqual(before, after)
        mirrored = game.var(client, "px", me), game.var(client, "py", me)
        self.assertLess(abs(mirrored[0] - after[0]) + abs(mirrored[1] - after[1]), 60)
        self.assertGreater(game.var(client, "ping"), 0)

    def test_shot_kills_player_scores_and_respawns(self):
        game = start(2)
        host = min(range(2), key=lambda i: game.cpus[i].net.slot)
        cpu = game.cpus[host]
        shooter = game.var(host, "me")
        victim = 1 - shooter
        cave = GLOBAL_BASE + OFFSETS["cave"]
        sx, sy = game.var(host, "px", shooter), game.var(host, "py", shooter)

        def put(name, entry, value, size=2):
            address = GLOBAL_BASE + OFFSETS[name] + entry * size
            if size == 1:
                cpu.memory[address] = value
            else:
                cpu.write_16(address, value & 0xFFFF)

        # Clear a column of rock above the shooter, face up and park the victim in it
        for tile_y in range(max((sy - 80) >> 4, 0), (sy >> 4) + 1):
            cpu.memory[cave + tile_y * 64 + (sx >> 4)] = 0
        put("pang", shooter, 0, 1)
        put("pinv", shooter, 0, 1)
        put("px", victim, sx)
        put("py", victim, sy - 48)
        put("pvx", victim, 0)
        put("pvy", victim, 0)
        put("pinv", victim, 0, 1)
        put("php", victim, 20, 1)
        game.keys[host] = 16
        game.frames(20)
        game.keys[host] = 0
        game.frames(2)
        self.assertEqual(game.var(host, "php", victim, 1), 0)
        self.assertEqual(game.var(host, "pkills", shooter, 1), 1)
        self.assertEqual(game.var(host, "pdeaths", victim, 1), 1)
        self.assertGreaterEqual(game.var(host, "pscore", shooter), 100)
        # The other instance mirrors the result and the respawn countdown
        other = 1 - host
        game.frames(4)
        self.assertEqual(game.var(other, "pdeaths", victim, 1), 1)
        self.assertGreaterEqual(game.var(other, "prt", victim), 9)
        game.frames(600)
        self.assertEqual(game.var(host, "php", victim, 1), 100)
        self.assertEqual(game.var(host, "pammo", victim, 1), 30)

    def test_turret_destroyed_awards_points_and_repairs_boosted(self):
        game = start(1)
        cpu = game.cpus[0]
        me = game.var(0, "me")
        nt = game.var(0, "nt")
        self.assertGreater(nt, 3)
        address = GLOBAL_BASE + OFFSETS["thp"]
        before = game.var(0, "pscore", me)
        tx, ty = game.var(0, "tx", 0), game.var(0, "ty", 0)
        for name, value in (("px", tx - 30), ("py", ty)):
            cpu.write_16(GLOBAL_BASE + OFFSETS[name] + me * 2, value)
        cpu.memory[address] = 1  # nearly dead
        cave = GLOBAL_BASE + OFFSETS["cave"]
        for x in range(tx - 40, tx - 4, 4):
            cpu.memory[cave + (ty >> 4) * 64 + (x >> 4)] = 0
        cpu.memory[GLOBAL_BASE + OFFSETS["pang"] + me] = 8  # facing right
        game.keys[0] = 16
        game.frames(15)
        game.keys[0] = 0
        self.assertGreaterEqual(game.var(0, "pscore", me), before + 50)
        game.frames(10)
        timer = game.var(0, "ttimer", 0)
        self.assertGreaterEqual(timer, 0)
        cpu.write_16(GLOBAL_BASE + OFFSETS["ttimer"], 2)
        game.frames(5)
        self.assertEqual(game.var(0, "thp", 0, 1), 60)
        self.assertEqual(game.var(0, "tboost", 0, 1), 1)

    def test_disconnect_keeps_scoreboard_entry_ten_seconds(self):
        game = start(2)
        host = min(range(2), key=lambda i: game.cpus[i].net.slot)
        leaver = 1 - host
        leaver_entry = next(t for t in range(8) if game.var(host, "pnet", t, 1) == game.cpus[leaver].net.slot
                            and game.var(host, "pused", t, 1) == 1)
        game.cpus[leaver].net.close()
        game.cpus.pop(leaver)
        game.keys.pop(leaver)
        game.frames(300)  # peer timeout 3.5 s is ~210 frames
        host = 0
        self.assertEqual(game.var(host, "pused", leaver_entry, 1), 2)
        game.frames(620)
        self.assertEqual(game.var(host, "pused", leaver_entry, 1), 0)

    def test_total_count_of_players_limited_to_eight(self):
        game = start(8, settle=80)
        used = sum(1 for t in range(8) if game.var(0, "pused", t, 1) == 1)
        self.assertEqual(used, 8)
        colours = {game.var(0, "pcol", t) for t in range(8)}
        self.assertEqual(len(colours), 8)

    def test_graphics_and_sound(self):
        game = start(2)
        cpu = game.cpus[0]
        colours = {pixel(cpu, x, y) for y in range(0, 240, 3) for x in range(0, 320, 3)}
        self.assertTrue({13, 14} & colours)
        self.assertIn(0, colours)


class TileMapTests(unittest.TestCase):
    def test_gfx_tilemap_draws_tiles_with_camera(self):
        source = """
        unsigned char map[4];
        int main(void) {
            map[0] = 2; map[1] = 3; map[2] = 4; map[3] = 5;
            while (1) { gfx_tilemap(map, 2, 2, 8, 0); gfx_present(); }
            return 0;
        }
        """
        cpu = make_cpu(c_compiler.compile_source(source, "tile.c"))
        run_frame(cpu)
        self.assertEqual(pixel(cpu, 0, 0), 2)      # first tile, scrolled by 8 px
        self.assertEqual(pixel(cpu, 7, 15), 2)
        self.assertEqual(pixel(cpu, 8, 0), 3)
        self.assertEqual(pixel(cpu, 0, 16), 4)
        self.assertEqual(pixel(cpu, 8, 16), 5)
        self.assertEqual(pixel(cpu, 40, 40), 0)    # outside the map

    def test_gfx_getchar_returns_and_consumes_key(self):
        source = """
        int got;
        int main(void) {
            while (1) { got = gfx_getchar(); gfx_present(); }
            return 0;
        }
        """
        cpu = make_cpu(c_compiler.compile_source(source, "char.c"))
        cpu.memory[CHAR] = ord("q")
        run_frame(cpu)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), ord("q"))
        self.assertEqual(cpu.memory[CHAR], 0)


if __name__ == "__main__":
    unittest.main()
