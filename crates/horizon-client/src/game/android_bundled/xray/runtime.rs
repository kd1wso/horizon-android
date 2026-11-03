// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// X-Ray: ores marked through blocks around the player, switched with a key.
// The sweep is bounded (`BUDGET` blocks a frame), so a phone never stalls on
// it; only complete sweeps are published, so the markers never flicker
// between passes. Settings are kept where the WebAssembly mod of the same id
// kept them (`horizon-mods/.horizon-mod-data/xray/enabled`, `radius`, `rare`).
use std::cell::RefCell;

/// The mod's id: its settings folder and its key's line.
pub const ID: &str = "xray";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str =
    "Mark ores through blocks around you; the key, radius and ores are yours to choose.";
/// The key's name, and its default.
pub const KEY: &str = "toggle";
pub const DEFAULT_KEY: &str = "key.keyboard.x";

/// The sweep radii, in blocks, and the one a fresh install starts with.
pub const RADII: [i32; 3] = [8, 16, 24];
pub const DEFAULT_RADIUS: usize = 1;
/// Blocks read a frame: a radius-16 sweep (35 937 blocks) finishes in about
/// 18 frames, a radius-24 one (117 649) in under four seconds.
pub const BUDGET: usize = 2048;
/// Markers a sweep keeps (the nearest ones).
pub const KEEP: usize = 96;
/// Markers a frame draws (the markers are sorted nearest first).
pub const DRAWN: usize = 64;

/// A block the sweep found, in world coordinates, with the colour to draw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub pos: [i32; 3],
    pub color: u32,
}

const DIAMOND: u32 = 0x00FF_E840;
const EMERALD: u32 = 0x0080_F030;
const GOLD: u32 = 0x0040_D8FF;
const REDSTONE: u32 = 0x0040_40FF;
const LAPIS: u32 = 0x00FF_8050;
const DEBRIS: u32 = 0x00D0_80FF;
const IRON: u32 = 0x00A0_C0E8;
const COPPER: u32 = 0x0040_90FF;
const COAL: u32 = 0x0090_9090;
const QUARTZ: u32 = 0x00FF_FFFF;

/// The colour of an ore block `name` (`minecraft:deepslate_gold_ore`), or
/// none for anything else. `rare_only` leaves out the common ores (coal,
/// copper, iron, quartz), as X-Ray mods do.
pub fn ore_color(name: &str, rare_only: bool) -> Option<u32> {
    let name = name.strip_prefix("minecraft:")?;
    let name = name.strip_prefix("deepslate_").unwrap_or(name);
    Some(match name {
        "diamond_ore" => DIAMOND,
        "emerald_ore" => EMERALD,
        "gold_ore" | "nether_gold_ore" => GOLD,
        "redstone_ore" => REDSTONE,
        "lapis_ore" => LAPIS,
        "ancient_debris" => DEBRIS,
        "iron_ore" if !rare_only => IRON,
        "copper_ore" if !rare_only => COPPER,
        "coal_ore" if !rare_only => COAL,
        "nether_quartz_ore" if !rare_only => QUARTZ,
        _ => return None,
    })
}

/// The settings, as the mod's own files keep them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Marking ores now (the key switches it; the mod itself is turned on in
    /// Options > Mods).
    pub active: bool,
    /// An index into `RADII`.
    pub radius: usize,
    /// Leave out coal, copper, iron and quartz.
    pub rare_only: bool,
}

impl Settings {
    /// From the stored values (`get` reads one key's file): marking on, the
    /// 16-block radius and every ore for the ones missing or unknown.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let radius = get("radius")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .and_then(|v| RADII.iter().position(|&r| r == v))
            .unwrap_or(DEFAULT_RADIUS);
        Self {
            active: get("enabled").is_none_or(|v| v.trim() != "false"),
            radius,
            rare_only: get("rare").is_some_and(|v| v.trim() == "true"),
        }
    }
    /// The settings page's buttons.
    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Mark ores: {}", on(self.active)),
            format!("Radius: {} blocks", RADII[self.radius.min(RADII.len() - 1)]),
            format!("Rare ores only: {}", on(self.rare_only)),
        ]
    }
    /// A click on button `i` of `labels`.
    pub fn click(&mut self, i: usize) {
        match i {
            0 => self.active = !self.active,
            1 => self.radius = (self.radius + 1) % RADII.len(),
            2 => self.rare_only = !self.rare_only,
            _ => {}
        }
    }
    /// What to store, key by key.
    pub fn stored(&self) -> [(&'static str, String); 3] {
        let flag = |b: bool| (if b { "true" } else { "false" }).to_owned();
        [
            ("enabled", flag(self.active)),
            ("radius", RADII[self.radius.min(RADII.len() - 1)].to_string()),
            ("rare", flag(self.rare_only)),
        ]
    }
    /// The radius in blocks, for a stored index that may be stale.
    pub fn radius_blocks(&self) -> i32 {
        RADII[self.radius.min(RADII.len() - 1)]
    }
}

/// Squared distance between two block positions.
pub fn distance_squared(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3)
        .map(|i| (i64::from(a[i]) - i64::from(b[i])).pow(2))
        .sum()
}

/// One sweep around `origin`, a slice at a time.
struct Search {
    origin: [i32; 3],
    radius: i32,
    dimension: &'static str,
    rare_only: bool,
    cursor: usize,
    pending: Vec<Marker>,
}

impl Search {
    fn new(origin: [i32; 3], radius: i32, dimension: &'static str, rare_only: bool) -> Self {
        Self {
            origin,
            radius,
            dimension,
            rare_only,
            cursor: 0,
            pending: Vec::new(),
        }
    }

    /// Reads up to `BUDGET` more blocks; true once the sweep is complete.
    fn step(&mut self, read: &mut impl FnMut([i32; 3]) -> Option<u32>) -> bool {
        let side = (self.radius * 2 + 1) as usize;
        let total = side * side * side;
        let end = (self.cursor + BUDGET).min(total);
        let reach = i64::from(self.radius).pow(2);
        let origin = self.origin;
        for i in self.cursor..end {
            let offset = [
                (i % side) as i32 - self.radius,
                ((i / side) % side) as i32 - self.radius,
                (i / (side * side)) as i32 - self.radius,
            ];
            if distance_squared(offset, [0; 3]) > reach {
                continue;
            }
            let pos = std::array::from_fn(|axis| origin[axis] + offset[axis]);
            if let Some(color) = read(pos) {
                self.pending.push(Marker { pos, color });
            }
        }
        self.cursor = end;
        // Bound the memory a coal-heavy sweep would otherwise take; the
        // nearest markers are the ones a completed sweep keeps anyway.
        if self.pending.len() > KEEP * 8 {
            self.pending
                .sort_unstable_by_key(|m| (distance_squared(m.pos, origin), m.pos));
            self.pending.truncate(KEEP);
        }
        end == total
    }
}

#[derive(Default)]
struct State {
    settings: Option<Settings>,
    search: Option<Search>,
    markers: Vec<Marker>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The settings, read once through `get`.
pub fn settings(get: impl Fn(&str) -> Option<String>) -> Settings {
    STATE.with_borrow_mut(|s| *s.settings.get_or_insert_with(|| Settings::read(get)))
}

/// Replaces the settings (the settings page changed them): the next sweep
/// follows them, and turning marking off drops the markers at once.
pub fn set_settings(settings: Settings) {
    STATE.with_borrow_mut(|s| {
        s.settings = Some(settings);
        if !settings.active {
            s.search = None;
            s.markers.clear();
        }
    });
}

/// The key: flips marking and answers the settings to keep.
pub fn toggle(get: impl Fn(&str) -> Option<String>) -> Settings {
    let mut settings = settings(get);
    settings.active = !settings.active;
    set_settings(settings);
    settings
}

/// Advances the sweep around `center` by a frame's work, reading each block
/// with `read` (its colour, or none when it isn't an ore). A move past the
/// radius, another dimension, radius or ore filter starts the sweep again.
pub fn scan(
    center: [i32; 3],
    dimension: &'static str,
    settings: Settings,
    mut read: impl FnMut([i32; 3]) -> Option<u32>,
) {
    if !settings.active {
        clear();
        return;
    }
    let radius = settings.radius_blocks();
    STATE.with_borrow_mut(|s| {
        let restart = match &s.search {
            None => true,
            Some(q) => {
                q.dimension != dimension
                    || q.radius != radius
                    || q.rare_only != settings.rare_only
                    || distance_squared(q.origin, center) > i64::from(radius).pow(2)
            }
        };
        if restart {
            s.search = Some(Search::new(
                center,
                radius,
                dimension,
                settings.rare_only,
            ));
            s.markers.clear();
        }
        let finished = match s.search.as_mut() {
            Some(q) => q.step(&mut read),
            None => false,
        };
        if finished {
            let mut markers = s
                .search
                .as_mut()
                .map(|q| std::mem::take(&mut q.pending))
                .unwrap_or_default();
            markers.sort_unstable_by_key(|m| (distance_squared(m.pos, center), m.pos));
            markers.truncate(KEEP);
            s.markers = markers;
            s.search = Some(Search::new(
                center,
                radius,
                dimension,
                settings.rare_only,
            ));
        }
    });
}

/// The markers of the last completed sweep, nearest first.
pub fn with_markers(f: impl FnOnce(&[Marker])) {
    STATE.with_borrow(|s| f(&s.markers));
}

/// Drops the sweep and its markers (the mod was turned off, the world left).
pub fn clear() {
    STATE.with_borrow_mut(|s| {
        s.search = None;
        s.markers.clear();
    });
}

/// The camera-facing faces of a block as camera-relative quads, each in both
/// windings: the see-through pipeline culls back faces, and the winding it
/// keeps depends on the frame, so both are given and one survives.
pub fn cube_quads(pos: [i32; 3], camera: [f64; 3], out: &mut Vec<[[f32; 3]; 4]>) {
    const PAD: f64 = 0.002;
    let lo = [
        f64::from(pos[0]) - PAD,
        f64::from(pos[1]) - PAD,
        f64::from(pos[2]) - PAD,
    ];
    let hi = [
        f64::from(pos[0]) + 1.0 + PAD,
        f64::from(pos[1]) + 1.0 + PAD,
        f64::from(pos[2]) + 1.0 + PAD,
    ];
    let center = [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 0.5,
        f64::from(pos[2]) + 0.5,
    ];
    const FACES: [([f64; 3], [[u8; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
        ([0.0, 0.0, -1.0], [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]]),
        ([1.0, 0.0, 0.0], [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]]),
        ([-1.0, 0.0, 0.0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]),
        ([0.0, 1.0, 0.0], [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]]),
        ([0.0, -1.0, 0.0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
    ];
    let corner = |c: [u8; 3]| {
        [
            if c[0] == 0 { lo[0] } else { hi[0] },
            if c[1] == 0 { lo[1] } else { hi[1] },
            if c[2] == 0 { lo[2] } else { hi[2] },
        ]
    };
    for (normal, corners) in FACES {
        let toward: f64 = (0..3)
            .map(|axis| (camera[axis] - center[axis]) * normal[axis])
            .sum();
        if toward <= 0.0 {
            continue;
        }
        let quad: [[f32; 3]; 4] = corners.map(|c| {
            let p = corner(c);
            [
                (p[0] - camera[0]) as f32,
                (p[1] - camera[1]) as f32,
                (p[2] - camera[2]) as f32,
            ]
        });
        out.push(quad);
        out.push([quad[0], quad[3], quad[2], quad[1]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    #[test]
    fn reads_stored_settings_and_round_trips_them() {
        assert_eq!(
            Settings::read(stored(&[])),
            Settings {
                active: true,
                radius: DEFAULT_RADIUS,
                rare_only: false
            }
        );
        let s = Settings::read(stored(&[
            ("enabled", "false\n"),
            ("radius", "8"),
            ("rare", "true"),
        ]));
        assert_eq!(
            s,
            Settings {
                active: false,
                radius: 0,
                rare_only: true
            }
        );
        // An unknown radius falls back instead of panicking.
        assert_eq!(Settings::read(stored(&[("radius", "12")])).radius, DEFAULT_RADIUS);
        let round: Vec<(&str, String)> = s.stored().into();
        let back: Vec<(&str, &str)> = round.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::read(stored(&back)), s);
    }

    #[test]
    fn buttons_cycle_through_the_settings() {
        let mut s = Settings {
            active: true,
            radius: RADII.len() - 1,
            rare_only: false,
        };
        s.click(0);
        s.click(1);
        s.click(2);
        assert_eq!(
            s,
            Settings {
                active: false,
                radius: 0,
                rare_only: true
            }
        );
        assert_eq!(
            s.labels(),
            ["Mark ores: OFF", "Radius: 8 blocks", "Rare ores only: ON"]
        );
    }

    #[test]
    fn colors_only_the_ores_the_vanilla_names_give() {
        assert_eq!(ore_color("minecraft:diamond_ore", true), Some(DIAMOND));
        assert_eq!(ore_color("minecraft:deepslate_diamond_ore", false), Some(DIAMOND));
        assert_eq!(ore_color("minecraft:ancient_debris", true), Some(DEBRIS));
        assert_eq!(ore_color("minecraft:nether_gold_ore", true), Some(GOLD));
        assert!(ore_color("minecraft:iron_ore", false).is_some());
        assert_eq!(ore_color("minecraft:iron_ore", true), None);
        assert_eq!(ore_color("minecraft:coal_ore", true), None);
        for name in [
            "minecraft:diamond_block",
            "minecraft:stone",
            "minecraft:deepslate",
            "mymod:diamond_ore",
            "diamond_ore",
        ] {
            assert_eq!(ore_color(name, false), None, "{name}");
        }
    }

    #[test]
    fn a_sweep_publishes_the_nearest_markers_only_once_complete() {
        reset();
        let center = [0, 0, 0];
        let settings = Settings {
            active: true,
            radius: 0,
            rare_only: false,
        };
        let ore = [3, 0, 0];
        let read = |pos: [i32; 3]| (pos == ore).then_some(DIAMOND);
        with_markers(|m| assert!(m.is_empty()));
        // Radius 8 is 17^3 blocks: two frames of `BUDGET` and a third that
        // completes the sweep.
        let dimension = "minecraft:overworld";
        scan(center, dimension, settings, read);
        with_markers(|m| assert!(m.is_empty(), "nothing before the sweep ends"));
        scan(center, dimension, settings, read);
        with_markers(|m| assert!(m.is_empty(), "nothing before the sweep ends"));
        scan(center, dimension, settings, read);
        with_markers(|m| assert_eq!(m, [Marker { pos: ore, color: DIAMOND }]));
    }

    #[test]
    fn turning_marking_off_and_teleporting_drop_the_markers() {
        reset();
        let center = [0, 0, 0];
        let on = Settings {
            active: true,
            radius: 0,
            rare_only: false,
        };
        let read = |pos: [i32; 3]| (pos == [1, 1, 1]).then_some(GOLD);
        let overworld = "minecraft:overworld";
        for _ in 0..3 {
            scan(center, overworld, on, read);
        }
        with_markers(|m| assert_eq!(m.len(), 1));
        // A teleport restarts the sweep around the new place.
        scan([200, 40, 0], overworld, on, read);
        with_markers(|m| assert!(m.is_empty()));
        // Back at the first place, a full sweep publishes again.
        for _ in 0..3 {
            scan(center, overworld, on, read);
        }
        with_markers(|m| assert_eq!(m.len(), 1));
        // So does another ore filter at the same place.
        scan(
            center,
            overworld,
            Settings {
                rare_only: true,
                ..on
            },
            read,
        );
        with_markers(|m| assert!(m.is_empty()));
        // And another dimension at the same coordinates.
        scan(center, "minecraft:the_nether", on, read);
        with_markers(|m| assert!(m.is_empty()));
        set_settings(Settings {
            active: false,
            ..on
        });
        with_markers(|m| assert!(m.is_empty()));
    }

    #[test]
    fn a_block_gives_the_faces_of_its_cube_that_face_the_camera() {
        let mut quads = Vec::new();
        // Looking at a corner: three faces are visible, each in two windings.
        // The corners are camera-relative (`[3.5; 3]` is subtracted).
        cube_quads([0, 0, 0], [3.5, 3.5, 3.5], &mut quads);
        assert_eq!(quads.len(), 6);
        for quad in &quads {
            for corner in quad {
                for value in corner {
                    assert!((-3.502..=-2.498).contains(value), "{corner:?}");
                }
            }
        }
        // From straight above only the top face is offered.
        quads.clear();
        cube_quads([0, 0, 0], [0.5, 4.5, 0.5], &mut quads);
        assert_eq!(quads.len(), 2);
        // A camera on the block's center sees no face of it.
        quads.clear();
        cube_quads([0, 0, 0], [0.5, 0.5, 0.5], &mut quads);
        assert!(quads.is_empty());
    }

    #[test]
    fn a_sweep_keeps_the_nearest_ores_when_there_are_many() {
        reset();
        let center = [0, 0, 0];
        let settings = Settings {
            active: true,
            radius: 0,
            rare_only: false,
        };
        let read = |pos: [i32; 3]| (distance_squared(pos, center) <= 64).then_some(COAL);
        for _ in 0..3 {
            scan(center, "minecraft:overworld", settings, read);
        }
        with_markers(|m| {
            assert_eq!(m.len(), KEEP);
            assert!(
                distance_squared(m[0].pos, center) <= distance_squared(m[m.len() - 1].pos, center)
            );
        });
    }
}

/// Reset profile-scoped state when Android returns from its launcher.
pub(super) fn reset() {
    STATE.with_borrow_mut(|state| *state = State::default());
}
