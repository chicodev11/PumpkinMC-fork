"""Runs scenarios.json on a vanilla server over RCON, writes vanilla_trace.txt."""
import json, socket, struct, sys, time

HOST, PORT, PASS = "127.0.0.1", 25575, "cmp"
D = sys.argv[1]


class Rcon:
    def __init__(self):
        self.s = socket.create_connection((HOST, PORT))
        self.i = 0
        self.cmd(PASS, 3)

    def cmd(self, text, kind=2):
        self.i += 1
        data = struct.pack("<ii", self.i, kind) + text.encode() + b"\0\0"
        self.s.sendall(struct.pack("<i", len(data)) + data)
        n = struct.unpack("<i", self._read(4))[0]
        body = self._read(n)
        return body[8:-2].decode()

    def _read(self, n):
        b = b""
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c:
                raise EOFError
            b += c
        return b


spec = json.load(open(f"{D}/scenarios.json"))
groups = {}
for c in spec["candidates"]:
    groups.setdefault(c.split("[")[0], []).append(c)

r = Rcon()


def gametime():
    return int(r.cmd("time query gametime").split()[-2])


def step(n=1):
    target = gametime() + n
    r.cmd(f"tick step {n}")
    while gametime() < target:
        time.sleep(0.01)


def probe(pos):
    x, y, z = pos
    for bid, states in groups.items():
        if "passed" in r.cmd(f"execute if block {x} {y} {z} {bid}"):
            for st in states:
                if st == bid or "passed" in r.cmd(f"execute if block {x} {y} {z} {st}"):
                    return st.replace("minecraft:", "")
            return bid.replace("minecraft:", "") + "[?]"
    return "?"


def entity(tag):
    res = r.cmd(f"data get entity @e[tag={tag},limit=1] Pos")
    if "[" not in res:
        return f"{tag}=none"
    nums = res[res.index("[") + 1 : res.rindex("]")].split(",")
    return f"{tag}=" + ",".join(f"{float(n.strip().rstrip('d')):.3f}" for n in nums)


def count(entity_type):
    """`type#n`: how many entities of the type are in the cleared area."""
    a = spec["area"]
    res = r.cmd(
        f"execute if entity @e[type={entity_type},x={a[0]},y={a[1]},z={a[2]},"
        f"dx={a[3] - a[0]},dy={a[4] - a[1] + 10},dz={a[5] - a[2]}]"
    )
    n = int(res.split()[-1]) if "count" in res.lower() else 0
    return f"{entity_type.replace('minecraft:', '')}#{n}"


a = spec["area"]
print(r.cmd(f"forceload add {a[0]} {a[2]} {a[3]} {a[5]}"))
print(r.cmd("tick freeze"))
print(r.cmd("time query gametime"))
out = []
for sc in spec["scenarios"]:
    for c in spec["clear"]:
        r.cmd(c)
    step(5)
    for c in sc["setup"]:
        res = r.cmd(c)
        if "Changed" not in res and "placed" not in res.lower():
            print("setup:", c, "->", res)
    for t in range(sc["ticks"]):
        for e in sc["events"]:
            if e["tick"] == t:
                for c in e["cmds"]:
                    r.cmd(c)
        row = (
            [probe(p) for p in sc["probes"]]
            + [entity(e) for e in sc.get("entities", [])]
            + [count(c) for c in sc.get("counts", [])]
        )
        out.append(f"{sc['name']} t{t}: " + " | ".join(row))
        step()
    print("done", sc["name"])
open(f"{D}/vanilla_trace.txt", "w").write("\n".join(out) + "\n")
