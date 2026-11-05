// VeinMiner: crate::game::android_veinminer. Sneak and break an ore or a log with the
// right tool (one that gets its drops: an iron pickaxe for diamond ore, an axe for a
// log) and the blocks of the same kind joined to it, by a face, an edge or a corner,
// break with it: the ore vein (stone and deepslate ores together), the tree's trunk.
//
// Each of them breaks as the player's own break does (`break_block_as_player`): the
// tool's drops (Fortune, Silk Touch), its wear, the experience, hunger, statistics, the
// world's protections and the other mods' `destroy_block`. That is the game's own code,
// which a module of crate::game may call. Their drops and experience then come out of the
// block broken. It stops before the tool would break, and in creative it does nothing.
// Server-side: in single player and in a world opened to LAN, for the guests too (their
// sneaking); not on someone else's server.
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};

use crate::game::Game;
use crate::game::native_hooks::NativeMod;
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use horizon_core::geometry::Vec3;

/// The mod's id: its row in Options > Mods, its settings' folder and its chat command.
pub(super) const ID: &str = "veinminer";
const TITLE: &str = "VeinMiner";
const VERSION: &str = "1.0.0";

// Its settings: `mode` is "sneak" (the default) or "always"; `limit` the most blocks one
// break takes, that block included; `ores`, `logs` and `hint` are "on" unless "off".
const MODE: &str = "mode";
const LIMIT: &str = "limit";
const ORES: &str = "ores";
const LOGS: &str = "logs";
const HINT: &str = "hint";
const LIMITS: [usize; 5] = [16, 32, 64, 128, 256];
const DEFAULT_LIMIT: usize = 64;

/// The vanilla ore tags: one of them is one vein, so stone and deepslate ores go
/// together (and a lit redstone ore is the same block). Other ores (`*_ore`, ancient
/// debris) take the same block only.
const ORE_TAGS: [&str; 8] = [
    "minecraft:coal_ores",
    "minecraft:copper_ores",
    "minecraft:iron_ores",
    "minecraft:gold_ores",
    "minecraft:redstone_ores",
    "minecraft:lapis_ores",
    "minecraft:diamond_ores",
    "minecraft:emerald_ores",
];

thread_local! {
    /// Set while the mod breaks a vein: each of those breaks calls `destroy_block` again.
    static MINING: Cell<bool> = const { Cell::new(false) };
}

/// Holds `MINING` while a vein breaks; lets it go after, also if a break panics.
struct Mining;

impl Mining {
    fn start() -> Option<Self> {
        (!MINING.replace(true)).then_some(Self)
    }
}

impl Drop for Mining {
    fn drop(&mut self) {
        MINING.set(false);
    }
}

/// What a vein is made of.
enum Kind {
    /// The blocks of an ore tag.
    Tag(&'static str),
    /// One block.
    Block(String),
}

impl Kind {
    /// The vein that the block `name` starts, if it starts one with these settings.
    fn of(game: &Game<'_>, name: &str) -> Option<Self> {
        let items = game.res.items();
        if on(game, ORES) {
            if let Some(tag) = ORE_TAGS.into_iter().find(|t| items.block_in_tag(t, name)) {
                return Some(Self::Tag(tag));
            }
            if name.ends_with("_ore") || name == "minecraft:ancient_debris" {
                return Some(Self::Block(name.to_owned()));
            }
        }
        (on(game, LOGS) && items.block_in_tag("minecraft:logs", name))
            .then(|| Self::Block(name.to_owned()))
    }

    fn holds(&self, game: &Game<'_>, name: &str) -> bool {
        match self {
            Self::Tag(tag) => game.res.items().block_in_tag(tag, name),
            Self::Block(block) => block == name,
        }
    }
}

fn on(game: &Game<'_>, key: &str) -> bool {
    game.native_setting(ID, key).as_deref() != Some("off")
}

fn always(game: &Game<'_>) -> bool {
    game.native_setting(ID, MODE).as_deref() == Some("always")
}

/// The acting player sneaks: the host, or the LAN guest whose break it is.
fn sneaking(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| p.shift_key_down)
}

fn limit(game: &Game<'_>) -> usize {
    game.native_setting(ID, LIMIT)
        .and_then(|v| v.parse().ok())
        .filter(|n| LIMITS.contains(n))
        .unwrap_or(DEFAULT_LIMIT)
}

fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

pub(super) fn row() -> NativeMod {
    NativeMod {
        id: ID.into(),
        title: format!("{TITLE} {VERSION}"),
        description: "Sneak and break an ore or a log with the right tool: the whole vein \
                      or trunk breaks with it."
            .into(),
        keys: Vec::new(),
    }
}

/// The `destroy_block` hook: the acting player breaks `pos`, which the game allowed.
/// The rest of its vein breaks first, nearest first; then the game breaks `pos`.
pub(super) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3]) {
    if !game.native_on(ID) || game.creative || !(sneaking(game) || always(game)) {
        return;
    }
    // One of the vein's own breaks: it is only that block.
    let Some(_mining) = Mining::start() else {
        return;
    };
    let name = game.native_block(pos);
    let Some(kind) = Kind::of(game, &name) else {
        return;
    };
    // Only a tool that gets the block's drops: no vein for a wooden pickaxe on iron ore.
    let Some(tool) = game.held().map(|s| s.item.clone()) else {
        return;
    };
    if !game.res.items().correct_for_drops(game.held(), &name) {
        return;
    }
    let blocks = vein(pos, limit(game) - 1, |at| {
        kind.holds(game, &game.native_block(at))
    });
    let first = game.next_entity;
    for at in blocks {
        if !tool_lasts(game, &tool) || game.break_block_as_player(at).is_err() {
            break;
        }
    }
    gather(game, first, pos);
}

/// What the vein's breaks dropped, the item entities and experience orbs from `first` on,
/// comes out of the block broken (`origin`): each at the place it had in its own block, so
/// none is left in a pocket of the rock.
fn gather(game: &mut Game<'_>, first: u32, origin: [i32; 3]) {
    let [x, y, z] = origin.map(f64::from);
    let place = |p: Vec3| {
        Vec3::new(
            x + p.x.rem_euclid(1.0),
            y + p.y.rem_euclid(1.0),
            z + p.z.rem_euclid(1.0),
        )
    };
    for item in game.dropped.iter_mut().filter(|e| e.id > first) {
        item.position = place(item.position);
        item.previous = item.position;
    }
    for orb in game.orbs.iter_mut().filter(|o| o.id > first) {
        orb.position = place(orb.position);
        orb.previous = orb.position;
    }
}

/// The blocks joined to `origin` (by a face, an edge or a corner, through such blocks)
/// that `holds`, nearest first: at most `most` of them, `origin` not counted.
fn vein(origin: [i32; 3], most: usize, mut holds: impl FnMut([i32; 3]) -> bool) -> Vec<[i32; 3]> {
    let mut found = Vec::new();
    let mut seen = HashSet::from([origin]);
    let mut next = VecDeque::from([origin]);
    while let Some([x, y, z]) = next.pop_front() {
        for [dx, dy, dz] in around() {
            if found.len() >= most {
                return found;
            }
            let at = [x + dx, y + dy, z + dz];
            if seen.insert(at) && holds(at) {
                found.push(at);
                next.push_back(at);
            }
        }
    }
    found
}

/// The 26 offsets to the blocks around one.
fn around() -> impl Iterator<Item = [i32; 3]> {
    (-1..=1)
        .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [dx, dy, dz])))
        .filter(|d| *d != [0, 0, 0])
}

/// The hand still holds `tool`, with wear left for one more block and for the first one
/// (the game breaks it after the vein): the mod never breaks the tool.
fn tool_lasts(game: &Game<'_>, tool: &str) -> bool {
    let Some(stack) = game.held().filter(|s| &*s.item == tool) else {
        return false;
    };
    match game.res.items().get(&stack.item) {
        Some(info) if info.max_damage > 0 => {
            let wear = info.tool_damage_per_block.unwrap_or(0);
            info.max_damage.saturating_sub(stack.damage) > wear.saturating_mul(2)
        }
        _ => true,
    }
}

/// A pickaxe with ores on, an axe with logs on: the tools that take a vein.
fn holds_tool(game: &Game<'_>) -> bool {
    let Some(item) = game.held().map(|s| &*s.item) else {
        return false;
    };
    let items = game.res.items();
    (on(game, ORES)
        && (items.item_in_tag("minecraft:pickaxes", item) || item.ends_with("_pickaxe")))
        || (on(game, LOGS) && (items.item_in_tag("minecraft:axes", item) || item.ends_with("_axe")))
}

pub(super) fn buttons(game: &Game<'_>) -> Vec<String> {
    let mode = if always(game) { "Always" } else { "Sneak" };
    vec![
        format!("Mode: {mode}"),
        format!("Max blocks: {}", limit(game)),
        format!("Ores: {}", on_off(on(game, ORES))),
        format!("Logs: {}", on_off(on(game, LOGS))),
        format!("HUD hint: {}", on_off(on(game, HINT))),
    ]
}

pub(super) fn click(game: &mut Game<'_>, button: usize) {
    match button {
        0 => {
            let mode = if always(game) { "sneak" } else { "always" };
            game.native_set_setting(ID, MODE, mode);
        }
        1 => {
            let now = limit(game);
            let i = LIMITS
                .iter()
                .position(|&n| n == now)
                .map_or(0, |i| (i + 1) % LIMITS.len());
            game.native_set_setting(ID, LIMIT, &LIMITS[i].to_string());
        }
        2..=4 => {
            let key = [ORES, LOGS, HINT][button - 2];
            let value = if on(game, key) { "off" } else { "on" };
            game.native_set_setting(ID, key, value);
        }
        _ => {}
    }
}

/// "VeinMiner" at the top left while sneaking with a pickaxe or an axe (in the sneak mode).
pub(super) fn hud(game: &mut Game<'_>, batch: &mut HudBatch, corners: &mut Corners) {
    if !game.native_on(ID)
        || !game.native_playing()
        || game.creative
        || !sneaking(game)
        || !holds_tool(game)
        || always(game)
        || !on(game, HINT)
    {
        return;
    }
    let text = format!("{TITLE}: up to {} blocks", limit(game));
    let width = game.native_text_width(&text) + 4.0;
    let (x, y) = corners.place(Corner::TopLeft, width, 12.0);
    game.native_fill(batch, [x, y, width, 12.0], 0x7800_0000);
    game.native_text(batch, &text, [x + 2.0, y + 2.0], 0x00FF_AA00, true);
}

/// `/veinminer` in chat: what the mod does now. True when it was this mod's command.
pub(super) fn command(game: &mut Game<'_>, command: &str) -> bool {
    if command.split_whitespace().next() != Some(ID) {
        return false;
    }
    let how = if always(game) {
        "Break"
    } else {
        "Sneak and break"
    };
    let what = match (on(game, ORES), on(game, LOGS)) {
        (true, true) => "an ore or a log",
        (true, false) => "an ore",
        (false, true) => "a log",
        (false, false) => {
            game.native_chat(&format!("{TITLE}: ores and logs are off (Options > Mods)."));
            return true;
        }
    };
    let line = format!(
        "{TITLE} {VERSION}: {how} {what} with the right tool: up to {} blocks break.",
        limit(game)
    );
    game.native_chat(&line);
    true
}
