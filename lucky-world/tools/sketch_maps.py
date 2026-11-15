#!/usr/bin/env python3
"""The sketch the three LUCKY WORLD maps were first drawn from.

The maps themselves are the files in data/maps/ (the terrain grids and the
province, river and landmark lists): edit those, not this. The game never
runs this script and never makes maps of its own.

This script holds the hand-placed shapes the maps were started from
(coastlines, mountain ranges, deserts, forests, lakes, rivers, landmarks)
and lays the province centres out evenly. Running it again OVERWRITES the
map files, so only use it to start over:

    python3 tools/sketch_maps.py data/maps            # write the maps
    python3 tools/sketch_maps.py data/maps --preview  # also write PNG previews
"""

import math
import os
import sys

W, H = 68, 42          # hex columns and rows ("odd-r": odd rows sit half a hex to the right)
ROW = math.sqrt(3) / 2  # row spacing, with hexes one unit wide


def pos(c, r):
    return (c + 0.5 * (r & 1), r * ROW)


def neighbours(c, r):
    if r & 1:
        d = [(-1, 0), (1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    else:
        d = [(-1, 0), (1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    for dc, dr in d:
        cc, rr = c + dc, r + dr
        if 0 <= cc < W and 0 <= rr < H:
            yield cc, rr


# ---------------------------------------------------------------- noise ----

def hash2(ix, iy, seed):
    n = (ix * 374761393 + iy * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 0xFFFF) / 65535.0


def value_noise(x, y, seed):
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b = hash2(ix, iy, seed), hash2(ix + 1, iy, seed)
    c, d = hash2(ix, iy + 1, seed), hash2(ix + 1, iy + 1, seed)
    return (a + (b - a) * sx) * (1 - sy) + (c + (d - c) * sx) * sy


def fbm(x, y, seed, scale=0.18):
    total, amp, f = 0.0, 1.0, scale
    for o in range(3):
        total += amp * (value_noise(x * f, y * f, seed + o * 17) - 0.5)
        amp *= 0.5
        f *= 2.1
    return total * 1.6  # about -1 .. 1


# --------------------------------------------------------------- shapes ----

def sd_polygon(x, y, pts):
    """Signed distance: positive inside."""
    inside = False
    best = 1e9
    n = len(pts)
    for i in range(n):
        (x1, y1), (x2, y2) = pts[i], pts[(i + 1) % n]
        if (y1 > y) != (y2 > y):
            if x < (x2 - x1) * (y - y1) / (y2 - y1) + x1:
                inside = not inside
        dx, dy = x2 - x1, y2 - y1
        t = max(0.0, min(1.0, ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy)))
        px, py = x1 + t * dx - x, y1 + t * dy - y
        best = min(best, math.hypot(px, py))
    return best if inside else -best


def sd_ellipse(x, y, cx, cy, rx, ry, turn=0.0):
    a = math.radians(turn)
    dx, dy = x - cx, y - cy
    u = dx * math.cos(a) + dy * math.sin(a)
    v = -dx * math.sin(a) + dy * math.cos(a)
    k = math.hypot(u / rx, v / ry)
    return (1 - k) * min(rx, ry)


def sd_line(x, y, pts, width):
    best = 1e9
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        dx, dy = x2 - x1, y2 - y1
        t = max(0.0, min(1.0, ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy)))
        best = min(best, math.hypot(x1 + t * dx - x, y1 + t * dy - y))
    return width - best


def shape_sd(x, y, s):
    kind = s[0]
    if kind == "poly":
        return sd_polygon(x, y, s[1])
    if kind == "ellipse":
        return sd_ellipse(x, y, *s[1:])
    if kind == "line":
        return sd_line(x, y, s[1], s[2])
    raise ValueError(kind)


# ----------------------------------------------------------------- maps ----
# Each map: land shapes (with how rough their coast is), continent regions
# (nearest seed wins), each continent's ground cover, painted features (later
# ones win), rivers and landmarks. All coordinates are in hex widths: x from
# 0 to 68, y from 0 to 36.

GRAND = {
    "id": "grand",
    "seed": 11,
    "land": [
        (("poly", [(8, 6), (15, 3.5), (24, 4.5), (31, 3), (40, 4), (49, 3.2), (57, 5.5), (61, 10),
                   (60, 15), (62, 20), (59, 26), (60, 30), (54, 32.5), (47, 31.5), (41, 33), (35, 30.5),
                   (29, 33), (21, 32.5), (15, 30), (10, 31), (8.5, 26), (9.5, 20), (8, 15), (9, 10)]), 1.4),
        (("ellipse", 3.4, 20, 1.7, 3.4, 10), 0.4),   # Westward Isle
        (("ellipse", 65.5, 15.5, 1.5, 2.9, -10), 0.4),  # Eastmark Isle
        (("ellipse", 33, 35.0, 3.0, 1.1, 0), 0.3),   # the Southern Key
        (("ellipse", 64.5, 3.0, 2.2, 1.2, 0), 0.3),  # Northcape Rock
        (("ellipse", 3.5, 32, 1.8, 1.3, 0), 0.3),    # the Gull Isles
    ],
    "sea": [  # bays cut into the land
        (("ellipse", 35.5, 31.5, 3.2, 2.2, 0), 0.6),   # the Sunward Gulf
        (("ellipse", 61.5, 22.5, 2.2, 3.0, 0), 0.5),   # Steppe Bay
        (("ellipse", 5.5, 9.5, 2.0, 2.0, 0), 0.5),
    ],
    "regions": [
        ("northern_wilds", [(18, 7), (32, 7.5), (47, 7)]),
        ("heartland", [(22, 17), (31, 19), (13, 19)]),
        ("eastern_steppe", [(46, 17), (54, 15), (55, 22)]),
        ("sunward_coast", [(14, 27), (25, 28), (34, 27.5)]),
        ("green_south", [(45, 27), (54, 28.5)]),
    ],
    "cover": {
        "northern_wilds": [("F", 0.0), ("T", 0.56)],
        "heartland": [("P", 0.0), ("F", 0.6)],
        "eastern_steppe": [("S", 0.0), ("P", 0.64)],
        "sunward_coast": [("D", 0.0), ("S", 0.46), ("P", 0.68)],
        "green_south": [("J", 0.0), ("P", 0.64)],
    },
    "features": [
        ("I", ("line", [(10, 4), (30, 3.6), (55, 5)], 1.3)),
        ("T", ("line", [(9, 6.5), (30, 6), (56, 7.5)], 1.6)),
        ("F", ("ellipse", 22, 9.5, 8, 2.2, 5)),
        ("F", ("ellipse", 44, 10, 6, 2.0, -5)),
        ("H", ("line", [(12, 13), (22, 14.5), (31, 13.5), (39, 15.5)], 1.9)),
        ("M", ("line", [(12, 13), (22, 14.5), (31, 13.5), (39, 15.5)], 1.0)),   # the Spine
        ("H", ("line", [(50, 24), (55, 29)], 1.8)),
        ("M", ("line", [(50, 24), (55, 29)], 0.9)),                               # the Red Teeth
        ("D", ("ellipse", 54, 13, 4, 2.4, 20)),                                    # the Salt Waste
        ("F", ("ellipse", 17, 22, 3.5, 2.2, 0)),
        ("H", ("ellipse", 33, 22.5, 2.6, 1.6, 0)),
        ("W", ("ellipse", 41, 24, 2.6, 1.7, 0)),
        ("W", ("ellipse", 57.5, 30.5, 2.4, 1.3, 0)),
        ("W", ("ellipse", 47.5, 30.5, 2.0, 1.0, 0)),
        ("o", ("ellipse", 26.5, 20, 2.4, 1.5, 15)),                               # Heart Lake
        ("o", ("ellipse", 46, 20.5, 1.4, 1.0, 0)),
        ("P", ("ellipse", 3.4, 20, 1.4, 3.0, 10)),
        ("F", ("ellipse", 65.5, 15.5, 1.2, 2.4, -10)),
    ],
    "rivers": [
        ("River Ister", [(30, 14.5), (28.5, 17.5), (27, 19.5)]),
        ("River Sunwater", [(26.5, 21.8), (26, 25), (23.5, 29), (22.5, 33)]),
        ("River Calder", [(38.5, 16.5), (41, 20), (43, 24.5), (44.5, 28), (44, 32.5)]),
        ("River Frost", [(21, 13), (19, 9), (15, 5), (14, 2.5)]),
        ("River Khan", [(47, 12), (52, 9.5), (58, 7), (62, 6)]),
        ("River Weald", [(13, 15), (11, 18.5), (8, 20)]),
    ],
    "landmarks": [
        ("great_tower", "The Spine Tower", (26, 14)),
        ("pyramid", "Tomb of the Salt Kings", (55, 13.5)),
        ("lighthouse", "Gulf Light", (35.5, 29.2)),
        ("temple", "Temple of the Lake", (24, 18.5)),
        ("arch", "The Frost Gate", (40, 6.5)),
    ],
}

ISLES = {
    "id": "isles",
    "seed": 23,
    "land": [
        # (shape, roughness). Big islands first, then the scattered ones.
        (("ellipse", 33, 5.5, 9.5, 3.6, 3), 1.0),      # Stormholm (big)
        (("ellipse", 13, 4, 4.0, 2.2, -10), 0.7),
        (("ellipse", 52, 4.5, 4.6, 2.3, 8), 0.7),
        (("ellipse", 61, 9.5, 2.2, 1.6, 0), 0.5),
        (("ellipse", 5, 9.5, 2.2, 1.7, 0), 0.5),
        (("ellipse", 14, 18, 7.5, 5.0, -20), 1.1),      # Jade (big)
        (("ellipse", 5, 23.5, 2.6, 2.0, 0), 0.6),
        (("ellipse", 25.5, 13.5, 2.6, 1.8, 30), 0.6),
        (("ellipse", 24, 23, 3.4, 2.2, 0), 0.7),
        (("ellipse", 34, 17.5, 4.0, 3.0, 10), 0.8),     # the middle isle
        (("ellipse", 52, 18.5, 6.8, 4.5, 15), 1.0),     # Ember (big)
        (("ellipse", 63.5, 17, 2.6, 2.8, 0), 0.6),
        (("ellipse", 43.5, 12, 2.6, 1.6, -20), 0.6),
        (("ellipse", 42.5, 23.5, 2.6, 2.0, 0), 0.6),
        (("ellipse", 60.5, 25.5, 3.4, 2.0, 25), 0.7),
        (("ellipse", 30, 31, 8.0, 3.2, -6), 1.0),       # Coral (big)
        (("ellipse", 14, 30.5, 4.0, 2.4, 10), 0.7),
        (("ellipse", 6, 33, 2.2, 1.6, 0), 0.5),
        (("ellipse", 47, 31, 4.5, 2.6, -10), 0.7),
        (("ellipse", 59, 33, 2.8, 1.6, 0), 0.5),
        (("ellipse", 22, 35, 1.8, 1.0, 0), 0.4),
        (("ellipse", 39.5, 35, 1.7, 1.0, 0), 0.4),
        (("ellipse", 3.5, 16, 1.4, 1.2, 0), 0.4),
        (("ellipse", 66, 30, 1.2, 1.4, 0), 0.4),
        (("ellipse", 21, 8.5, 1.4, 1.0, 0), 0.4),
        (("ellipse", 46, 8, 1.3, 1.0, 0), 0.4),
        (("ellipse", 35.5, 25.5, 1.5, 1.1, 0), 0.4),
    ],
    "sea": [
        (("ellipse", 14, 18.5, 2.0, 1.4, 0), 0.4),       # Jade's inner lagoon
    ],
    "regions": [
        ("storm_isles", [(13, 4), (33, 5), (52, 4.5), (61, 9.5), (5, 9.5), (21, 8.5), (46, 8)]),
        ("jade_islands", [(14, 18), (5, 23.5), (25.5, 13.5), (24, 23), (3.5, 16), (34, 17.5)]),
        ("ember_atolls", [(52, 18.5), (63.5, 17), (43.5, 12), (42.5, 23.5), (60.5, 25.5)]),
        ("coral_chain", [(30, 31), (14, 30.5), (6, 33), (47, 31), (59, 33), (22, 35), (39.5, 35), (66, 30), (35.5, 25.5)]),
    ],
    "cover": {
        "storm_isles": [("T", 0.0), ("F", 0.5)],
        "jade_islands": [("P", 0.0), ("F", 0.56)],
        "ember_atolls": [("D", 0.0), ("S", 0.5)],
        "coral_chain": [("J", 0.0), ("P", 0.6)],
    },
    "features": [
        ("M", ("line", [(28, 5.5), (37, 5)], 0.9)),
        ("H", ("line", [(26, 6), (39, 5.5)], 1.6)),
        ("M", ("ellipse", 10, 16, 1.6, 1.2, 0)),
        ("H", ("ellipse", 18, 21, 2.0, 1.4, 0)),
        ("M", ("ellipse", 53, 17.5, 1.5, 1.4, 0)),       # the Ember volcano
        ("D", ("ellipse", 50, 20.5, 2.6, 1.5, 15)),
        ("W", ("ellipse", 30, 32, 2.0, 1.0, 0)),
        ("W", ("ellipse", 16, 31, 1.2, 0.8, 0)),
        ("H", ("ellipse", 34, 17, 1.3, 1.0, 0)),
        ("F", ("ellipse", 52, 4.5, 2.2, 1.2, 0)),
        ("I", ("ellipse", 34, 3.4, 3.5, 0.9, 0)),
    ],
    "rivers": [
        ("River Jade", [(10.5, 16.5), (13, 19.5), (16, 23)]),
        ("River Tern", [(31, 5.5), (32, 8.8)]),
        ("River Cinder", [(53, 18.5), (56.5, 21.5)]),
        ("River Palm", [(27, 31), (26.5, 33.6)]),
    ],
    "landmarks": [
        ("lighthouse", "Storm Light", (24.5, 6)),
        ("colossus", "The Jade Colossus", (16.5, 15)),
        ("temple", "Fire Temple", (54.5, 16.5)),
        ("arch", "Coral Arch", (34.5, 30)),
        ("great_tower", "Tern Watch", (40, 5)),
    ],
}

FIVE = {
    "id": "five_shores",
    "seed": 37,
    "land": [
        (("poly", [(12, 3), (22, 1.8), (36, 2.5), (48, 1.6), (58, 3), (60, 6.5), (52, 9.5), (41, 9),
                   (30, 10), (19, 9.5), (11, 7)]), 1.2),                                         # Norvane
        (("poly", [(3, 12.5), (11, 11.8), (19, 13), (22, 17), (20, 22), (14, 25.5), (7, 25), (3, 20)]), 1.2),  # Aurel
        (("poly", [(6, 28.5), (15, 27.8), (26, 28.5), (31, 31), (27, 35), (16, 35.5), (8, 34.5)]), 1.1),       # Verdane
        (("poly", [(40, 13), (50, 12), (60, 13), (65, 18), (64, 25), (60, 31), (52, 34.5), (44, 33),
                   (39, 27), (38, 19)]), 1.4),                                                      # Sahrun
        (("ellipse", 28, 14.5, 2.4, 1.6, 20), 0.6),                                                # Kesh Isles
        (("ellipse", 31.5, 19.5, 2.8, 1.8, -10), 0.6),
        (("ellipse", 27, 24, 2.2, 1.6, 0), 0.6),
        (("ellipse", 33.5, 25.5, 1.6, 1.2, 0), 0.5),
        (("ellipse", 64.5, 4.5, 1.6, 1.4, 0), 0.4),
        (("ellipse", 2.5, 30, 1.2, 1.6, 0), 0.4),
    ],
    "sea": [
        (("ellipse", 51, 23.5, 3.0, 2.2, 0), 0.0),  # Sahrun's inland sea is a lake (painted below)
    ],
    "regions": [
        ("norvane", [(20, 5), (35, 5.5), (50, 5), (64.5, 4.5)]),
        ("aurel", [(9, 16), (15, 20), (14, 14)]),
        ("verdane", [(12, 31), (22, 31.5), (2.5, 30)]),
        ("sahrun", [(46, 17), (57, 18), (48, 28), (58, 27)]),
        ("kesh_isles", [(28, 14.5), (31.5, 19.5), (27, 24), (33.5, 25.5)]),
    ],
    "cover": {
        "norvane": [("F", 0.0), ("T", 0.5)],
        "aurel": [("P", 0.0), ("F", 0.56)],
        "verdane": [("J", 0.0), ("W", 0.68)],
        "sahrun": [("D", 0.0), ("S", 0.6)],
        "kesh_isles": [("P", 0.0), ("F", 0.56)],
    },
    "features": [
        ("I", ("line", [(16, 2.6), (34, 2.6), (54, 2.6)], 1.1)),
        ("M", ("line", [(24, 6), (33, 7.2), (42, 6.5)], 0.9)),
        ("H", ("line", [(22, 6.5), (44, 6.5)], 1.6)),
        ("M", ("line", [(10, 15), (12, 21)], 0.9)),
        ("H", ("ellipse", 17, 18, 2.2, 1.6, 0)),
        ("M", ("line", [(58, 15), (62, 22)], 1.0)),
        ("H", ("line", [(57, 14), (63, 24)], 1.8)),
        ("S", ("ellipse", 44, 30, 4, 2.4, 0)),
        ("P", ("ellipse", 51, 23.5, 4.6, 3.6, 0)),
        ("o", ("ellipse", 51, 23.5, 2.6, 1.8, 0)),       # the Oasis Sea
        ("W", ("ellipse", 22, 31, 2.6, 1.6, 0)),
        ("F", ("ellipse", 10, 31, 2.8, 1.8, 0)),
        ("H", ("ellipse", 31.5, 19.5, 1.2, 0.9, 0)),
    ],
    "rivers": [
        ("River Norr", [(33, 7.5), (33, 10.2)]),
        ("River Aur", [(11, 19.5), (16.5, 23.5), (17, 26.5)]),
        ("River Sah", [(55, 23.5), (52.5, 23.5)]),
        ("River Mosi", [(47, 25), (44, 29), (44, 34.5)]),
        ("River Verd", [(16, 32), (14, 36)]),
    ],
    "landmarks": [
        ("pyramid", "The Great Pyramid", (46, 20.5)),
        ("lighthouse", "Kesh Light", (30, 18.5)),
        ("great_tower", "Norr Spire", (36, 7.5)),
        ("temple", "Temple of Verd", (18, 32.5)),
        ("colossus", "The Aurel Colossus", (8, 13.2)),
        ("arch", "Oasis Gate", (55.5, 20)),
    ],
}

STYLES = {
    "north": (["Frost", "Skar", "Hald", "Vint", "Isk", "Kald", "Brann", "Sval", "Tor", "Ulv", "Grim", "Ran", "Hvit", "Jarn"],
              ["mark", "holm", "fell", "vik", "heim", "dal", "stad", "fjord", "moor", "gard", "by", "nes"]),
    "middle": (["Val", "Aur", "Bel", "Lor", "Mon", "Cas", "Ver", "Alt", "Dor", "Sel", "Ros", "Fen", "Ash", "Wil"],
               ["mont", "shire", "ford", "vale", "bury", "ton", "wick", "field", "haven", "more", "mere", "brook"]),
    "east": (["Kesh", "Tar", "Zhan", "Ura", "Khor", "Bai", "Sar", "Tem", "Oz", "Kara", "Altyn", "Bek", "Yer", "Tol"],
             ["stan", "gol", "kent", "abad", "kul", "tash", "zhar", "bulak", "su", "tau", "orda", "sai"]),
    "sun": (["Sah", "Mir", "Qas", "Zan", "Ash", "Kha", "Dar", "Sul", "Ras", "Tam", "Jab", "Wad", "Nim", "Fal"],
            ["ar", "un", "iya", "ah", "esh", "ir", "abad", "oum", "ara", "ena", "ib", "ul"]),
    "south": (["Ver", "Ama", "Tula", "Coa", "Ixi", "Pana", "Sela", "Mara", "Oro", "Yuca", "Quet", "Chal", "Tepa", "Nahu"],
              ["ndo", "ca", "lan", "ria", "tepec", "pa", "co", "huac", "ma", "tlan", "na", "zal"]),
}

CONTINENT_STYLE = {
    "northern_wilds": "north", "heartland": "middle", "eastern_steppe": "east", "sunward_coast": "sun",
    "green_south": "south", "coral_chain": "south", "storm_isles": "north", "jade_islands": "middle",
    "ember_atolls": "sun", "norvane": "north", "aurel": "middle", "sahrun": "sun", "verdane": "south",
    "kesh_isles": "east",
}

LAND = set("PFJDSTMHWI")


def nearest_cell(x, y):
    r = max(0, min(H - 1, round(y / ROW)))
    best = None
    for rr in range(max(0, r - 1), min(H, r + 2)):
        c = round(x - 0.5 * (rr & 1))
        for cc in range(max(0, c - 1), min(W, c + 2)):
            px, py = pos(cc, rr)
            d = (px - x) ** 2 + (py - y) ** 2
            if best is None or d < best[0]:
                best = (d, cc, rr)
    return best[1], best[2]


def build(m):
    seed = m["seed"]
    grid = [["~"] * W for _ in range(H)]
    region = [[None] * W for _ in range(H)]
    for r in range(H):
        for c in range(W):
            x, y = pos(c, r)
            land = max(shape_sd(x, y, s) + rough * 1.4 * fbm(x, y, seed) for s, rough in m["land"])
            for s, rough in m["sea"]:
                if shape_sd(x, y, s) + rough * fbm(x, y, seed + 5) > 0:
                    land = -1
            # Nothing on the map's very edge.
            if c == 0 or r == 0 or c == W - 1 or r == H - 1:
                land = -1
            if land > 0:
                best = None
                for cont, seeds in m["regions"]:
                    for sx, sy in seeds:
                        d = math.hypot(sx - x, sy - y) + 1.2 * fbm(x, y, seed + 9, 0.25)
                        if best is None or d < best[0]:
                            best = (d, cont)
                region[r][c] = best[1]
                cover = m["cover"][best[1]]
                n = 0.5 + 0.5 * fbm(x, y, seed + 31, 0.3)
                letter = cover[0][0]
                for t, at in cover:
                    if n >= at:
                        letter = t
                grid[r][c] = letter
    # Painted features, later ones on top (only on land; lakes only inland).
    for letter, s in m["features"]:
        for r in range(H):
            for c in range(W):
                if grid[r][c] in LAND and shape_sd(*pos(c, r), s) + 0.35 * fbm(*pos(c, r), seed + 77) > 0:
                    grid[r][c] = letter
    # Shallow water next to land.
    for r in range(H):
        for c in range(W):
            if grid[r][c] == "~" and any(grid[rr][cc] in LAND for cc, rr in neighbours(c, r)):
                grid[r][c] = "-"
    return grid, region


def components(cells_ok):
    seen, out = set(), []
    for start in cells_ok:
        if start in seen:
            continue
        comp, stack = [], [start]
        seen.add(start)
        while stack:
            cell = stack.pop()
            comp.append(cell)
            for nb in neighbours(*cell):
                if nb in cells_ok and nb not in seen:
                    seen.add(nb)
                    stack.append(nb)
        out.append(comp)
    return out


def provinces(grid, region, per=10):
    out = []
    by_region = {}
    for r in range(H):
        for c in range(W):
            if grid[r][c] in LAND:
                by_region.setdefault(region[r][c], set()).add((c, r))
    for cont in sorted(by_region):
        for comp in sorted(components(by_region[cont]), key=lambda k: min(k)):
            k = max(1, round(len(comp) / per))
            pts = [pos(*cell) for cell in comp]
            # Farthest-point start, then Lloyd steps: evenly spread centres.
            centres = [pts[0]]
            while len(centres) < k:
                far = max(pts, key=lambda p: min((p[0] - q[0]) ** 2 + (p[1] - q[1]) ** 2 for q in centres))
                centres.append(far)
            for _ in range(12):
                groups = [[] for _ in centres]
                for p in pts:
                    i = min(range(len(centres)), key=lambda j: (p[0] - centres[j][0]) ** 2 + (p[1] - centres[j][1]) ** 2)
                    groups[i].append(p)
                centres = [(sum(p[0] for p in g) / len(g), sum(p[1] for p in g) / len(g)) if g else centres[i]
                           for i, g in enumerate(groups)]
            for cx, cy in centres:
                cell = min(comp, key=lambda cell: (pos(*cell)[0] - cx) ** 2 + (pos(*cell)[1] - cy) ** 2)
                out.append((cont, cell))
    return out


def name_maker(seed):
    used = set()

    def make(style):
        starts, ends = STYLES[style]
        for i in range(400):
            h = hash2(len(used) * 7 + i, seed, 3)
            g = hash2(len(used) * 13 + i, seed, 5)
            name = starts[int(h * len(starts)) % len(starts)] + ends[int(g * len(ends)) % len(ends)]
            if name not in used:
                used.add(name)
                return name
        raise RuntimeError("out of names")
    return make


def river_cells(points):
    cells = []
    for (x1, y1), (x2, y2) in zip(points, points[1:]):
        steps = max(2, int(math.hypot(x2 - x1, y2 - y1) * 4))
        for i in range(steps + 1):
            t = i / steps
            cell = nearest_cell(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t)
            if not cells or cells[-1] != cell:
                cells.append(cell)
    return cells


def write(m, out_dir, preview):
    grid, region = build(m)
    provs = provinces(grid, region)
    make = name_maker(m["seed"])
    with open(os.path.join(out_dir, m["id"] + ".txt"), "w") as f:
        for row in grid:
            f.write("".join(row) + "\n")
    lines = [
        f"# {m['id']}: provinces, rivers and landmarks. The terrain is {m['id']}.txt beside this file.",
        "#",
        "# A province is every land hex nearer (walking over land) to its `at` hex than to",
        "# any other province's. Move `at` to move its borders; add or remove provinces freely.",
        "# `at` is [column, row] in the terrain grid, counting from 0 at the top left.",
        "# `continent` must be one of this map's continents in data/maps.toml.",
        "",
    ]
    ids = set()
    for cont, (c, r) in provs:
        name = make(CONTINENT_STYLE[cont])
        pid = name.lower()
        ids.add(pid)
        lines += ["[[province]]", f'id = "{pid}"', f'name = "{name}"', f'continent = "{cont}"', f"at = [{c}, {r}]", ""]
    for name, pts in m["rivers"]:
        cells = river_cells(pts)
        # Keep the river to its land hexes and its mouth (the first water hex
        # after land): a river flowing out of a lake starts on its shore.
        while cells and grid[cells[0][1]][cells[0][0]] not in LAND:
            cells = cells[1:]
        for i, (c, r) in enumerate(cells):
            if grid[r][c] not in LAND:
                cells = cells[: i + 1]
                break
        lines += ["[[river]]", f'name = "{name}"',
                  "cells = [" + ", ".join(f"[{c}, {r}]" for c, r in cells) + "]", ""]
    for kind, name, (x, y) in m["landmarks"]:
        c, r = nearest_cell(x, y)
        if grid[r][c] not in LAND or grid[r][c] == "I":
            land_cells = [(cc, rr) for rr in range(H) for cc in range(W) if grid[rr][cc] in LAND - {"I"}]
            c, r = min(land_cells, key=lambda cell: (pos(*cell)[0] - x) ** 2 + (pos(*cell)[1] - y) ** 2)
            print(f"  note: {name} moved onto land at {c},{r}")
        lines += ["[[landmark]]", f'kind = "{kind}"', f'name = "{name}"', f"at = [{c}, {r}]", ""]
    with open(os.path.join(out_dir, m["id"] + ".toml"), "w") as f:
        f.write("\n".join(lines))
    land = sum(ch in LAND for row in grid for ch in row)
    print(f"{m['id']}: {land} land hexes, {len(provs)} provinces")
    if preview:
        draw_preview(grid, provs, m, os.path.join(out_dir, "..", "..", "out", m["id"] + "-sketch.png"))


COLORS = {"~": (24, 52, 110), "-": (52, 104, 168), "o": (70, 140, 200), "P": (150, 190, 90), "F": (60, 120, 60),
          "J": (40, 110, 50), "D": (225, 200, 130), "S": (190, 180, 110), "T": (190, 200, 190), "M": (130, 115, 100),
          "H": (150, 150, 100), "W": (90, 130, 110), "I": (240, 245, 250)}


def draw_preview(grid, provs, m, path):
    from PIL import Image, ImageDraw
    k = 12
    im = Image.new("RGB", (int((W + 1) * k), int((H + 1) * ROW * k)), (0, 0, 0))
    d = ImageDraw.Draw(im)
    for r in range(H):
        for c in range(W):
            x, y = pos(c, r)
            d.ellipse([x * k, y * k, x * k + k, y * k + k], fill=COLORS[grid[r][c]])
    for _, (c, r) in provs:
        x, y = pos(c, r)
        d.ellipse([x * k + 3, y * k + 3, x * k + k - 3, y * k + k - 3], fill=(255, 0, 0))
    for _, pts in m["rivers"]:
        for (c, r) in river_cells(pts):
            x, y = pos(c, r)
            d.ellipse([x * k + 4, y * k + 4, x * k + k - 4, y * k + k - 4], fill=(0, 200, 255))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    im.save(path)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "data/maps"
    for m in (GRAND, ISLES, FIVE):
        write(m, out, "--preview" in sys.argv)
