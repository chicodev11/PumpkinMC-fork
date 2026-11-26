"""Redstone timing scenarios shared by the vanilla (RCON) and Pumpkin (tester) runners.

Writes scenarios.json: each scenario has setup commands, events at given ticks and probe
positions. Runners do: setup; then for t in 0..ticks: run events at t, probe, step one tick.
"""
import json

O = (54, 100, -9)
AREA = (49, 99, -15, 62, 106, -2)


def p(dx, dy, dz):
    return (O[0] + dx, O[1] + dy, O[2] + dz)


def s(pos):
    return f"{pos[0]} {pos[1]} {pos[2]}"


def setb(dx, dy, dz, state):
    return f"setblock {s(p(dx, dy, dz))} {state}"


CANDIDATES = []
for b in ["piston", "sticky_piston"]:
    for f in ["north", "south", "east", "west", "up", "down"]:
        for e in ["true", "false"]:
            CANDIDATES.append(f"minecraft:{b}[facing={f},extended={e}]")
for f in ["north", "south", "east", "west", "up", "down"]:
    CANDIDATES.append(f"minecraft:piston_head[facing={f}]")
    for pw in ["true", "false"]:
        CANDIDATES.append(f"minecraft:observer[facing={f},powered={pw}]")
for f in ["north", "south", "east", "west"]:
    for pw in ["true", "false"]:
        CANDIDATES.append(f"minecraft:repeater[facing={f},powered={pw}]")
        CANDIDATES.append(f"minecraft:redstone_wall_torch[facing={f},lit={pw}]")
for pw in ["true", "false"]:
    CANDIDATES.append(f"minecraft:redstone_lamp[lit={pw}]")
    CANDIDATES.append(f"minecraft:redstone_torch[lit={pw}]")
for pw in range(16):
    CANDIDATES.append(f"minecraft:redstone_wire[power={pw}]")
for lvl in range(9):
    CANDIDATES.append(f"minecraft:composter[level={lvl}]")
for b0 in ["true", "false"]:
    for b1 in ["true", "false"]:
        for b2 in ["true", "false"]:
            CANDIDATES.append(
                f"minecraft:brewing_stand[has_bottle_0={b0},has_bottle_1={b1},has_bottle_2={b2}]"
            )
for lit in ["true", "false"]:
    CANDIDATES.append(f"minecraft:furnace[lit={lit}]")
for c in ["true", "false"]:
    for t in ["true", "false"]:
        CANDIDATES.append(f"minecraft:crafter[crafting={c},triggered={t}]")
        CANDIDATES.append(f"minecraft:crafter[crafting={c},orientation=up_north,triggered={t}]")
for r in ["true", "false"]:
    CANDIDATES.append(f"minecraft:jukebox[has_record={r}]")
CANDIDATES += [
    "minecraft:dead_tube_coral_fan",
    "minecraft:dead_tube_coral_wall_fan",
    "minecraft:pumpkin",
    "minecraft:white_carpet",
    "minecraft:rail",
    "minecraft:torch",
    "minecraft:lever",
    "minecraft:oak_sign",
    "minecraft:decorated_pot",
    "minecraft:chiseled_bookshelf",
    "minecraft:moving_piston",
    "minecraft:air",
    "minecraft:stone",
    "minecraft:redstone_block",
    "minecraft:slime_block",
    "minecraft:honey_block",
    "minecraft:glass",
    "minecraft:obsidian",
    "minecraft:sand",
    "minecraft:chest",
    "minecraft:tnt",
    "minecraft:sugar_cane",
    "minecraft:dead_brain_coral_wall_fan",
    "minecraft:hopper[enabled=true]",
    "minecraft:hopper[enabled=false]",
    "minecraft:dropper[triggered=true]",
    "minecraft:dropper[triggered=false]",
    "minecraft:powered_rail[powered=true]",
    "minecraft:detector_rail[powered=true,shape=north_south]",
    "minecraft:detector_rail[powered=false,shape=north_south]",
    "minecraft:powered_rail[powered=false]",
]
for f in ["north", "south", "east", "west"]:
    for m in ["compare", "subtract"]:
        for pw in ["true", "false"]:
            CANDIDATES.append(f"minecraft:comparator[facing={f},mode={m},powered={pw}]")

scenarios = []


def scenario(name, setup, events, probes, ticks, entities=(), counts=()):
    """`entities`: tags of entities whose position is traced every tick as well.
    `counts`: entity types counted in the area every tick (TNT dupers, farm drops)."""
    scenarios.append(
        {
            "name": name,
            "setup": setup,
            "events": [{"tick": t, "cmds": c} for t, c in events],
            "probes": [list(p(*q)) for q in probes],
            "ticks": ticks,
            "entities": list(entities),
            "counts": list(counts),
        }
    )


def stand(tag, dx, dy, dz):
    """An armor stand at the centre of a block's bottom face."""
    x, y, z = p(dx, dy, dz)
    return f'summon armor_stand {x + 0.5} {y} {z + 0.5} {{Tags:["{tag}"]}}'


# 1. Repeater line (delays 1, 2, 4) into a lamp; on at 0, off at 14
scenario(
    "repeater_line",
    [
        setb(1, 0, 0, "repeater[facing=west,delay=1]"),
        setb(2, 0, 0, "repeater[facing=west,delay=2]"),
        setb(3, 0, 0, "repeater[facing=west,delay=4]"),
        setb(4, 0, 0, "redstone_lamp"),
    ],
    [(0, [setb(0, 0, 0, "redstone_block")]), (14, [setb(0, 0, 0, "air")])],
    [(1, 0, 0), (2, 0, 0), (3, 0, 0), (4, 0, 0)],
    32,
)

# 2. Piston push and retract
scenario(
    "piston_push_retract",
    [setb(0, 0, 0, "piston[facing=east]"), setb(1, 0, 0, "stone")],
    [(0, [setb(0, 0, -1, "redstone_block")]), (6, [setb(0, 0, -1, "air")])],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0)],
    14,
)

# 3. Observer pulse into a sticky piston (spits its block)
scenario(
    "observer_sticky_spit",
    [
        setb(0, 0, 0, "sticky_piston[facing=east]"),
        setb(1, 0, 0, "stone"),
        setb(0, 0, -1, "observer[facing=north]"),
    ],
    [(0, [setb(0, 0, -2, "stone")])],
    [(0, 0, -1), (0, 0, 0), (1, 0, 0), (2, 0, 0)],
    12,
)

# 4. Two pistons race for the same space, powered from one wire
scenario(
    "piston_race",
    [
        setb(0, 0, 0, "piston[facing=east]"),
        setb(1, 0, 0, "stone"),
        setb(3, 0, 0, "piston[facing=west]"),
        setb(4, 0, 0, "glass"),
    ]
    + [setb(x, 0, -2, "redstone_wire") for x in range(0, 4)]
    + [setb(0, 0, -1, "redstone_wire"), setb(3, 0, -1, "redstone_wire")],
    [(0, [setb(1, 0, -3, "redstone_block")])],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0), (0, 0, -1), (3, 0, -1)],
    8,
)

# 5. Observer clock
scenario(
    "observer_clock",
    [],
    [(0, [setb(0, 0, 0, "observer[facing=east]"), setb(1, 0, 0, "observer[facing=west]")])],
    [(0, 0, 0), (1, 0, 0)],
    16,
)

# 6. Quasi-connectivity: powered diagonally, extends only after a neighbour update
scenario(
    "qc_bud",
    [setb(0, 0, 0, "piston[facing=up]")],
    [(0, [setb(1, 1, 0, "redstone_block")]), (3, [setb(-1, 0, 0, "stone")])],
    [(0, 0, 0), (0, 1, 0)],
    8,
)

# 7. Piston pushes a redstone block that powers a second piston
scenario(
    "piston_chain",
    [
        setb(0, 0, 0, "piston[facing=east]"),
        setb(1, 0, 0, "redstone_block"),
        setb(2, 0, 1, "piston[facing=south]"),
        setb(2, 0, 2, "stone"),
    ],
    [(0, [setb(0, 0, -1, "redstone_block")]), (6, [setb(0, 0, -1, "air")])],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0), (2, 0, 1), (2, 0, 2), (2, 0, 3)],
    14,
)

# 8. Torch inverter with a repeater feedback loop (clock)
scenario(
    "torch_repeater_clock",
    [
        setb(0, 0, 0, "stone"),
        setb(1, 0, 0, "redstone_wall_torch[facing=east]"),
        setb(1, 0, 1, "repeater[facing=north,delay=1]"),
        setb(0, 0, 1, "redstone_wire"),
        setb(-1, 0, 1, "redstone_wire"),
        setb(-1, 0, 0, "repeater[facing=south,delay=1]"),
    ],
    [(0, [setb(1, 1, 0, "glass")])],
    [(1, 0, 0), (1, 0, 1), (-1, 0, 0), (0, 0, 1)],
    20,
)

# 9. Sticky piston pulled by a 1-tick torch pulse through a slime block
scenario(
    "slime_double_push",
    [
        setb(0, 0, 0, "sticky_piston[facing=east]"),
        setb(1, -1, 0, "air"),
        setb(1, 0, 0, "slime_block"),
        setb(1, 0, 1, "stone"),
        setb(1, 0, -1, "stone"),
        setb(2, 0, 0, "stone"),
    ],
    [(0, [setb(0, 0, -1, "redstone_block")]), (1, [setb(0, 0, -1, "air")])],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0), (2, 0, 1), (2, 0, -1), (1, 0, 1)],
    10,
)

# 10-13. The race at other positions and along z: the wire's HashSet update order is locational
def race(name, ox, oz, axis):
    def q(a, b):
        return (ox + a, 0, oz + b) if axis == "x" else (ox + b, 0, oz + a)
    f1, f2 = ("east", "west") if axis == "x" else ("south", "north")
    cmds = [
        setb(*q(0, 0), f"piston[facing={f1}]"),
        setb(*q(1, 0), "stone"),
        setb(*q(3, 0), f"piston[facing={f2}]"),
        setb(*q(4, 0), "glass"),
    ]
    cmds += [setb(*q(a, -2), "redstone_wire") for a in range(0, 4)]
    cmds += [setb(*q(0, -1), "redstone_wire"), setb(*q(3, -1), "redstone_wire")]
    scenario(name, cmds, [(0, [setb(*q(1, -3), "redstone_block")])],
             [q(0, 0), q(1, 0), q(2, 0), q(3, 0)], 5)

race("race_shift_x1", 1, 0, "x")
race("race_shift_x3", 3, 2, "x")
race("race_z", -2, 0, "z")
race("race_z_shift", 2, 1, "z")

# 14. Comparator reading a chest (set with NBT), side input in compare and subtract mode
CHEST = 'chest{Items:[{Slot:0b,id:"minecraft:stone",count:64},{Slot:1b,id:"minecraft:stone",count:64},' \
    '{Slot:2b,id:"minecraft:stone",count:64},{Slot:3b,id:"minecraft:stone",count:64}]}'
for mode in ["compare", "subtract"]:
    scenario(
        f"comparator_{mode}",
        [
            setb(1, 0, 0, f"comparator[facing=west,mode={mode}]"),
            setb(2, 0, 0, "redstone_wire"),
            setb(1, 0, -1, "redstone_wire"),
            setb(1, 0, -2, "redstone_wire"),
        ],
        [
            (0, [setb(0, 0, 0, CHEST)]),
            (5, [setb(1, 0, -3, "redstone_block")]),
            (10, [setb(1, 0, -3, "air")]),
        ],
        [(1, 0, 0), (2, 0, 0), (1, 0, -1)],
        16,
    )

# 15. Two hoppers passing one item back and forth, read by comparators
scenario(
    "hopper_ping_pong",
    [
        setb(0, 0, 0, 'hopper[facing=east]{Items:[{Slot:0b,id:"minecraft:stone",count:1}]}'),
        setb(1, 0, 0, "hopper[facing=west]"),
        setb(0, 0, -1, "comparator[facing=south]"),
        setb(0, 0, -2, "redstone_wire"),
        setb(1, 0, 1, "comparator[facing=north]"),
        setb(1, 0, 2, "redstone_wire"),
    ],
    [],
    [(0, 0, -2), (1, 0, 2), (0, 0, -1), (1, 0, 1)],
    40,
)

# 16. Dropper pulses into a chest, read by a comparator
scenario(
    "dropper_into_chest",
    [
        setb(0, 0, 0, 'dropper[facing=east]{Items:[{Slot:0b,id:"minecraft:stone",count:5}]}'),
        setb(1, 0, 0, "chest"),
        setb(2, 0, 0, "comparator[facing=west]"),
        setb(3, 0, 0, "redstone_wire"),
    ],
    [
        (0, [setb(0, 1, 0, "redstone_block")]),
        (1, [setb(0, 1, 0, "air")]),
        (8, [setb(0, 1, 0, "redstone_block")]),
        (9, [setb(0, 1, 0, "air")]),
    ],
    [(0, 0, 0), (2, 0, 0), (3, 0, 0)],
    18,
)

# 17. Powered rail line (9 rails reach 8 blocks from the source)
scenario(
    "powered_rail_line",
    [setb(x, 0, 0, "powered_rail[shape=east_west]") for x in range(-4, 7)],
    [(0, [setb(-5, 0, 0, "redstone_block")]), (6, [setb(-5, 0, 0, "air")])],
    [(-4, 0, 0), (0, 0, 0), (4, 0, 0), (5, 0, 0), (6, 0, 0)],
    10,
)

# 18. TNT lit by a placed redstone block, and TNT pushed next to power
scenario(
    "tnt_ignition",
    [
        setb(0, 0, 0, "tnt"),
        setb(3, 0, 0, "piston[facing=east]"),
        setb(4, 0, 0, "tnt"),
        setb(6, 0, 0, "redstone_block"),
    ],
    [(0, [setb(0, 0, -1, "redstone_block")]), (2, [setb(3, 0, -1, "redstone_block")])],
    [(0, 0, 0), (4, 0, 0), (5, 0, 0)],
    8,
)

# 19-23. Pistons moving entities (`Entity.move(PISTON)`, slime launch, honey, moving block
# collision). Kept a block above the floor, which slime and honey would pull along.
scenario(
    "piston_push_entity",
    [setb(0, 1, 0, "piston[facing=east]"), setb(0, 0, 0, "stone"), stand("e1", 1, 1, 0)],
    [(5, [setb(-1, 1, 0, "redstone_block")]), (12, [setb(-1, 1, 0, "air")])],
    [(1, 1, 0)],
    20,
    ["e1"],
)
scenario(
    "piston_push_into_wall",
    [
        setb(0, 1, 0, "piston[facing=east]"),
        setb(2, 1, 0, "stone"),
        setb(1, 0, 0, "glass"),
        stand("e1", 1, 1, 0),
    ],
    [(5, [setb(-1, 1, 0, "redstone_block")])],
    [(1, 1, 0)],
    12,
    ["e1"],
)
scenario(
    "slime_launch",
    [setb(0, 1, 0, "piston[facing=up]"), setb(0, 2, 0, "slime_block"), stand("e1", 0, 3, 0)],
    [(5, [setb(-1, 1, 0, "redstone_block")])],
    [(0, 2, 0), (0, 3, 0)],
    25,
    ["e1"],
)
scenario(
    "piston_elevator",
    [setb(0, 1, 0, "piston[facing=up]"), setb(0, 2, 0, "stone"), stand("e1", 0, 3, 0)],
    [(5, [setb(-1, 1, 0, "redstone_block")]), (12, [setb(-1, 1, 0, "air")])],
    [(0, 2, 0), (0, 3, 0)],
    20,
    ["e1"],
)
scenario(
    "honey_carry",
    [setb(0, 2, 0, "piston[facing=east]"), setb(1, 2, 0, "honey_block"), stand("e1", 1, 3, 0)],
    [(5, [setb(-1, 2, 0, "redstone_block")])],
    [(1, 2, 0), (2, 2, 0)],
    12,
    ["e1"],
)

# 24. TNT duplication. The TNT is powered without an update (`setblock ... strict`, a BUD
# state). The push turns the slime the wall fan hangs on into a moving block; the fan pops
# with a neighbour update while the TNT is still at its old position, so the TNT primes there
# and the moving block still carries a TNT block forward.
scenario(
    "tnt_dupe_coral",
    [
        setb(0, 1, 0, "piston[facing=east]"),
        setb(1, 1, 0, "slime_block"),
        setb(2, 1, 0, "slime_block"),
        setb(2, 2, 0, "slime_block"),
        setb(2, 2, -1, "tnt"),
        setb(2, 1, -1, "dead_brain_coral_wall_fan[facing=north]"),
        setb(2, 3, -1, "redstone_block strict"),
    ],
    [(3, [setb(-1, 1, 0, "redstone_block")])],
    [(2, 2, -1), (3, 2, -1), (2, 1, -1), (3, 1, 0)],
    20,
    counts=["minecraft:tnt", "minecraft:item"],
)

# 25. The same push without the fan: the BUD TNT just moves (no dupe).
scenario(
    "tnt_bud_push_no_fan",
    [
        setb(0, 1, 0, "piston[facing=east]"),
        setb(1, 1, 0, "slime_block"),
        setb(2, 1, 0, "slime_block"),
        setb(2, 2, 0, "slime_block"),
        setb(2, 2, -1, "tnt"),
        setb(2, 3, -1, "redstone_block strict"),
    ],
    [(3, [setb(-1, 1, 0, "redstone_block")])],
    [(2, 2, -1), (3, 2, -1)],
    12,
    counts=["minecraft:tnt"],
)

# 26-27. Push limit: a sticky piston pulls a slime block with 11 blocks stuck to it (12
# moved), then the same with one more block (13: nothing moves).
def slime_wall(extra):
    cmds = [setb(0, 1, 0, "sticky_piston[facing=east]"), setb(1, 1, 0, "slime_block")]
    stuck = [(1, 2, 0), (1, 3, 0), (1, 1, 1), (1, 2, 1), (1, 3, 1), (1, 1, -1), (1, 2, -1),
             (1, 3, -1), (1, 4, 0), (1, 4, 1), (1, 4, -1)]
    if extra:
        stuck.append((1, 5, 0))
    return cmds + [setb(*q, "slime_block") for q in stuck]


for name, extra in [("push_limit_12", False), ("push_limit_13", True)]:
    scenario(
        name,
        slime_wall(extra),
        [(0, [setb(-1, 1, 0, "redstone_block")]), (6, [setb(-1, 1, 0, "air")])],
        [(1, 1, 0), (2, 1, 0), (2, 4, 0), (1, 4, -1), (2, 4, -1)],
        14,
    )

# 28. Slime and honey side by side do not stick to each other: each drags only its own
# neighbours, so the two halves split.
scenario(
    "slime_honey_split",
    [
        setb(0, 1, 0, "sticky_piston[facing=east]"),
        setb(1, 1, 0, "slime_block"),
        setb(1, 2, 0, "honey_block"),
        setb(1, 1, 1, "stone"),
        setb(1, 2, 1, "stone"),
        setb(1, 3, 0, "glass"),
    ],
    [(0, [setb(-1, 1, 0, "redstone_block")]), (6, [setb(-1, 1, 0, "air")])],
    [(1, 1, 0), (2, 1, 0), (1, 2, 0), (2, 2, 0), (1, 1, 1), (2, 1, 1), (1, 2, 1), (2, 2, 1)],
    14,
)

# 29. Observer sugar cane farm: the cane grows (set by command; growth is random), the
# observer above it sees the change, its pulse runs through dust onto the stone over the
# piston, which breaks the cane. The drops are counted.
scenario(
    "observer_cane_farm",
    [
        setb(0, 0, 0, "sand"),
        setb(1, 0, 0, "water"),
        setb(2, 0, 0, "stone"),
        setb(1, 0, 1, "stone"),
        setb(1, 0, -1, "stone"),
        setb(0, 1, 0, "sugar_cane"),
        setb(-1, 2, 0, "piston[facing=east]"),
        setb(-1, 3, 0, "stone"),
        setb(-1, 4, 0, "redstone_wire"),
        setb(0, 3, 0, "observer[facing=down]"),
        setb(0, 4, 0, "redstone_wire"),
    ],
    [(2, [setb(0, 2, 0, "sugar_cane")]), (16, [setb(0, 2, 0, "sugar_cane")])],
    [(0, 1, 0), (0, 2, 0), (-1, 2, 0), (0, 3, 0), (-1, 4, 0)],
    30,
    counts=["minecraft:item"],
)

# Fourth batch: containers behind hoppers (upstream's code since the 2026-11-16 merge). A hopper
# feeds the container from above (and the side), another drains it from below; a comparator on
# the east side reads it into a wire.


def item(id_, count, slot, extra=""):
    return f'{{Slot:{slot}b,id:"minecraft:{id_}",count:{count}{extra}}}'


def hopper(facing, items):
    return f"hopper[facing={facing}]{{Items:[{','.join(items)}]}}"


def container_scenario(name, target, top, side, ticks, drain=True):
    setup = [
        setb(1, 0, 0, "comparator[facing=west]"),
        setb(2, 0, 0, "redstone_wire"),
    ] + ([setb(0, -1, 0, "hopper[facing=east]")] if drain else [])
    events = [(0, [setb(0, 0, 0, target)] + ([setb(0, 1, 0, top)] if top else [])
               + ([setb(0, 0, -1, side)] if side else []))]
    scenario(name, setup, events, [(0, 0, 0), (1, 0, 0), (2, 0, 0)], ticks)


container_scenario(
    "hopper_composter",
    "composter",
    hopper("down", [item("pumpkin_pie", 9, 0)]),
    None,
    90,
)
WATER = ',components:{"minecraft:potion_contents":{potion:"minecraft:water"}}'
container_scenario(
    "hopper_brewing_stand",
    "brewing_stand",
    hopper("down", [item("nether_wart", 2, 0)]),
    hopper("south", [item("blaze_powder", 1, 0), item("potion", 1, 1, WATER),
                     item("potion", 1, 2, WATER), item("glass_bottle", 2, 3)]),
    40,
    drain=False,
)
container_scenario(
    "hopper_furnace",
    "furnace",
    hopper("down", [item("raw_iron", 2, 0)]),
    hopper("south", [item("coal", 1, 0)]),
    230,
)
container_scenario(
    "hopper_crafter",
    "crafter",
    hopper("down", [item("oak_planks", 12, 0)]),
    None,
    70,
    drain=False,
)
container_scenario(
    "hopper_chiseled_bookshelf",
    "chiseled_bookshelf",
    hopper("down", [item("book", 4, 0), item("enchanted_book", 1, 1)]),
    None,
    50,
)
container_scenario(
    "hopper_jukebox",
    "jukebox",
    hopper("down", [item("music_disc_cat", 1, 0), item("music_disc_13", 1, 1)]),
    None,
    40,
)
container_scenario(
    "hopper_decorated_pot",
    "decorated_pot",
    hopper("down", [item("stone", 5, 0), item("dirt", 2, 1)]),
    None,
    50,
    drain=False,
)

# Long container runs: a full smelt drained from below, brewing to the end, a crafter pulse.
container_scenario(
    "hopper_furnace_smelt",
    "furnace",
    hopper("down", [item("raw_iron", 2, 0)]),
    hopper("south", [item("coal", 1, 0)]),
    440,
)
container_scenario(
    "hopper_brewing_finish",
    "brewing_stand",
    hopper("down", [item("nether_wart", 2, 0)]),
    hopper("south", [item("blaze_powder", 1, 0), item("potion", 3, 1, WATER)]),
    440,
    drain=False,
)
scenario(
    "crafter_pulse",
    [
        setb(1, 0, 0, "comparator[facing=west]"),
        setb(2, 0, 0, "redstone_wire"),
    ],
    [
        (0, [setb(0, 0, 0, "crafter[orientation=up_north]{Items:["
                           + item("oak_planks", 1, 0) + "," + item("oak_planks", 1, 3) + "]}")]),
        (5, [setb(0, 0, 1, "redstone_block")]),
        (6, [setb(0, 0, 1, "air")]),
        (12, [setb(0, 0, 1, "redstone_block")]),
        (13, [setb(0, 0, 1, "air")]),
    ],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0)],
    25,
    counts=["minecraft:item"],
)

# Fifth batch: container entities and a jukebox drained by a hopper.
container_scenario(
    "hopper_jukebox_drain",
    "jukebox",
    hopper("down", [item("music_disc_cat", 1, 0), item("music_disc_13", 1, 1)]),
    None,
    60,
)


def summon(dx, dy, dz, entity, nbt=""):
    x, y, z = p(dx, dy, dz)
    return f"summon minecraft:{entity} {x + 0.5} {y} {z + 0.5} {nbt}".rstrip()


# A hopper minecart on a detector rail pulls a chest above it empty; the comparator behind the
# rail reads the cart.
scenario(
    "hopper_minecart_from_chest",
    [
        setb(0, 0, 0, "detector_rail[shape=north_south]"),
        setb(1, 0, 0, "comparator[facing=west]"),
        setb(2, 0, 0, "redstone_wire"),
    ],
    [
        (0, [setb(0, 1, 0, CHEST.replace("count:64", "count:3")), summon(0, 0, 0, "hopper_minecart")]),
    ],
    [(0, 0, 0), (1, 0, 0), (2, 0, 0)],
    60,
)
# A chest boat resting on a hopper is emptied into the chest beside it.
scenario(
    "chest_boat_into_hopper",
    [
        setb(0, 0, 0, "hopper[facing=east]"),
        setb(1, 0, 0, "chest"),
        setb(2, 0, 0, "comparator[facing=west]"),
        setb(3, 0, 0, "redstone_wire"),
    ],
    [
        (0, [summon(0, 1, 0, "oak_chest_boat",
                    '{Items:[{Slot:0b,id:"minecraft:stone",count:5}]}')]),
    ],
    [(0, 0, 0), (2, 0, 0), (3, 0, 0)],
    80,
)

# Update order: sticky pistons given 0-tick and 1-tick pulses (block spitting), and an observer
# clock feeding a sticky piston.
for name, off_tick in [("sticky_zero_tick", 2), ("sticky_one_tick", 3), ("sticky_two_tick", 4)]:
    scenario(
        name,
        [setb(0, 0, 0, "sticky_piston[facing=east]"), setb(1, 0, 0, "stone")],
        [(2, [setb(-1, 0, 0, "redstone_block")] + ([setb(-1, 0, 0, "air")] if off_tick == 2 else [])),
         (off_tick, [setb(-1, 0, 0, "air")])],
        [(0, 0, 0), (1, 0, 0), (2, 0, 0)],
        12,
    )
scenario(
    "observer_clock_sticky",
    [
        setb(0, 0, 0, "observer[facing=east]"),
        setb(1, 0, 0, "observer[facing=west]"),
        setb(-1, 0, 0, "sticky_piston[facing=north]"),
        setb(-1, 0, -1, "glass"),
    ],
    # Flipping the first observer's state starts the pair pulsing.
    [(1, [setb(0, 0, 0, "observer[facing=east,powered=true]")])],
    [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (-1, 0, -1), (-1, 0, -2)],
    30,
)

# Slime engines from the wiki's two-way engine A (2x2, observers on top, one block above the floor so the slime does not stick to it), all four observer and
# piston direction readings: the halves push and observers fire on being moved.
for obs_facing in ["up", "down"]:
    for p1, p2 in [("west", "east"), ("east", "west")]:
        trigger = (
            [(2, [setb(0, 3, 0, "stone")]), (3, [setb(0, 3, 0, "air")])]
            if obs_facing == "up"
            else [(2, [setb(0, 2, 0, "observer[facing=down,powered=true]")])]
        )
        scenario(
            f"slime_engine_{obs_facing}_{p1}",
            [
                setb(0, 1, 0, "slime_block"),
                setb(1, 1, 0, f"sticky_piston[facing={p1}]"),
                setb(0, 1, 1, f"sticky_piston[facing={p2}]"),
                setb(1, 1, 1, "slime_block"),
                setb(0, 2, 0, f"observer[facing={obs_facing}]"),
                setb(1, 2, 1, f"observer[facing={obs_facing}]"),
            ],
            trigger,
            [(x, y, z) for x in (-1, 0, 1, 2, 3) for z in (0, 1) for y in (1, 2)],
            20,
        )

# Ported from the dev branch (parallel 2026-11-15 session).
# breaks in the shape updates after the TNT is gone: no dupe.
scenario(
    "tnt_dupe",
    [
        setb(0, 1, 0, "sticky_piston[facing=east]"),
        setb(1, 1, 0, "slime_block"),
        setb(2, 1, 0, "slime_block"),
        setb(2, 1, 1, "slime_block"),
        setb(2, 2, 1, "tnt"),
        setb(2, 2, 0, "dead_tube_coral_fan"),
        setb(2, 3, 1, "redstone_block strict"),
    ],
    [(2, [setb(-1, 1, 0, "redstone_block")]), (8, [setb(-1, 1, 0, "air")])],
    [(2, 2, 1), (3, 2, 1), (2, 2, 0), (3, 2, 0), (2, 1, 0), (3, 1, 0)],
    16,
    counts=["minecraft:tnt"],
)
scenario(
    "tnt_dupe_control",
    [
        setb(0, 1, 0, "sticky_piston[facing=up]"),
        setb(0, 2, 0, "slime_block"),
        setb(0, 2, -1, "tnt"),
        setb(1, 2, 0, "slime_block"),
        setb(1, 2, -1, "dead_tube_coral_wall_fan[facing=north]"),
        setb(0, 2, -2, "redstone_block strict"),
    ],
    [(2, [setb(-1, 1, 0, "redstone_block")])],
    [(0, 2, -1), (0, 3, -1), (1, 2, -1)],
    8,
    counts=["minecraft:tnt"],
)

# 26. Observer pumpkin farm: the observer sees the pumpkin, the piston breaks it, the hopper takes
# the item and a comparator reads the hopper.
scenario(
    "observer_pumpkin_farm",
    [
        setb(0, 1, 0, "piston[facing=east]"),
        setb(1, 0, 0, "hopper"),
        setb(1, 0, -1, "stone"),
        setb(1, 0, -2, "stone"),
        setb(0, 0, -2, "stone"),
        setb(0, 0, -1, "stone"),
        setb(1, 1, -1, "observer[facing=south]"),
        setb(1, 1, -2, "redstone_wire"),
        setb(0, 1, -2, "redstone_wire"),
        setb(0, 1, -1, "redstone_wire"),
        setb(1, 0, 1, "comparator[facing=south]"),
        setb(1, 0, 2, "redstone_wire"),
    ],
    [(2, [setb(1, 1, 0, "pumpkin")]), (12, [setb(1, 1, 0, "pumpkin")])],
    [(1, 1, 0), (1, 1, -1), (0, 1, -1), (0, 1, 0), (1, 0, 1), (1, 0, 2)],
    24,
    counts=["minecraft:item"],
)

# 27. Blocks riding a slime structure land on slime that is still a moving block in the same tick:
# vanilla counts an arrived moving piston as its block (`getCollisionShape`), so they stay.
scenario(
    "slime_riders",
    [
        setb(0, 1, 0, "sticky_piston[facing=east]"),
        setb(1, 1, 0, "slime_block"),
        setb(1, 1, 1, "slime_block"),
        setb(1, 1, -1, "slime_block"),
        setb(1, 1, -2, "slime_block"),
        setb(1, 1, 2, "slime_block"),
        setb(1, 2, 0, "white_carpet"),
        setb(1, 2, 1, "rail"),
        setb(1, 2, -1, "torch"),
        setb(1, 2, -2, "lever[face=floor]"),
        setb(1, 2, 2, "oak_sign"),
    ],
    [(2, [setb(-1, 1, 0, "redstone_block")]), (8, [setb(-1, 1, 0, "air")])],
    [(1, 2, z) for z in (-2, -1, 0, 1, 2)] + [(2, 2, z) for z in (-2, -1, 0, 1, 2)],
    16,
    counts=["minecraft:item"],
)

out = {
    "area": list(AREA),
    "clear": [
        "kill @e[type=minecraft:tnt]",
        "kill @e[type=minecraft:armor_stand]",
        f"fill {AREA[0]} {AREA[1]} {AREA[2]} {AREA[3]} {AREA[4]} {AREA[5]} air",
        f"fill {AREA[0]} {AREA[1]} {AREA[2]} {AREA[3]} {AREA[1]} {AREA[5]} stone",
        # Launched entities fly above the area; the terrain there differs between the two worlds.
        f"fill {AREA[0]} {AREA[4] + 1} {AREA[2]} {AREA[3]} {AREA[4] + 10} {AREA[5]} air",
        "kill @e[type=minecraft:hopper_minecart]",
        "kill @e[type=minecraft:oak_chest_boat]",
        "kill @e[type=minecraft:item]",
    ],
    "candidates": CANDIDATES,
    "scenarios": scenarios,
}
json.dump(out, open(__file__.replace("scenarios.py", "scenarios.json"), "w"), indent=1)
print(len(scenarios), "scenarios,", len(CANDIDATES), "candidates")
