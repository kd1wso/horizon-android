//! The launcher's look: a pixel night with a castle in violet fog behind every page, dark
//! translucent plates, violet controls whose corners are cut in pixel steps, and the pixel wordmark
//! beside a cube (`docs/android.md`, "Launcher look"). It is all plain quads through [`Ui`], with
//! no textures and nothing baked, laid out in the launcher's dp.
use horizon_client::ui_theme::{Face, Ui};

/// The launcher's palette in the HUD's colour encoding; the game's overlays keep
/// `ui_theme::color`.
pub(super) mod color {
    pub use horizon_client::ui_theme::color::hex;
    /// Under everything: the night's darkest blue.
    pub const BG: u32 = hex(0x04050f, 255);
    pub const TEXT: u32 = hex(0xf5f3ff, 255);
    /// Secondary text, the pages' idle tabs.
    pub const MUTED: u32 = hex(0x9a97b4, 255);
    /// Text read on plates: the steps.
    pub const SOFT: u32 = hex(0xd8d5ea, 255);
    pub const ACCENT: u32 = hex(0x7432d8, 255);
    /// The wordmark's "Android", selections, the focus ring.
    pub const ACCENT_HI: u32 = hex(0xa66bff, 255);
    pub const ON_ACCENT: u32 = hex(0xffffff, 255);
    /// Secondary buttons: a dark plate under a fine outline.
    pub const RAISED: u32 = hex(0x11132b, 235);
    /// A switch that is off, the skin's silhouette.
    pub const HOVER: u32 = hex(0x2b2752, 255);
    /// Rules between rows, the scroll track.
    pub const BORDER: u32 = hex(0x9aa0ff, 30);
    /// Outlines of plates and controls.
    pub const STROKE: u32 = hex(0x9aa0ff, 50);
    /// The pages' plate, over the night.
    pub const CARD: u32 = hex(0x0a0c21, 222);
    /// Dialogs and panels: nearly opaque.
    pub const PANEL: u32 = hex(0x0d0f27, 250);
    /// Rows on a plate (worlds, the skin's panel).
    pub const ROW: u32 = hex(0xb9acff, 14);
    pub const SUCCESS: u32 = hex(0x5fd39a, 255);
    /// Behind dialogs.
    pub const VEIL: u32 = hex(0x020309, 170);
    /// Over a control while it is pressed.
    pub const PRESSED: u32 = hex(0xc8b8ff, 44);
}
use color::hex;

/// `a` to `b` by `t`, channel by channel (alpha too).
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let mut out = 0;
    for shift in [0, 8, 16, 24] {
        let (x, y) = ((a >> shift & 0xff) as f32, (b >> shift & 0xff) as f32);
        out |= ((x + (y - x) * t).round() as u32) << shift;
    }
    out
}

/// A rectangle whose corners are cut in `steps` square steps of `s` dp, shaded from `top` to
/// `bottom`.
pub(super) fn stepped(
    ui: &mut Ui,
    [x, y, w, h]: [f32; 4],
    s: f32,
    steps: u32,
    top: u32,
    bottom: u32,
) {
    let n = steps as f32;
    let at = |dy: f32| mix(top, bottom, dy / h.max(1.0));
    for i in 0..steps {
        let inset = (n - i as f32) * s;
        let y0 = i as f32 * s;
        ui.gradient(x + inset, y + y0, w - 2.0 * inset, s, at(y0), at(y0 + s));
        let y1 = h - (i + 1) as f32 * s;
        ui.gradient(x + inset, y + y1, w - 2.0 * inset, s, at(y1), at(y1 + s));
    }
    ui.gradient(x, y + n * s, w, h - 2.0 * n * s, at(n * s), at(h - n * s));
}

/// A soft halo of `rgb` around a rectangle: rings of light that fade outwards.
pub(super) fn glow(
    ui: &mut Ui,
    [x, y, w, h]: [f32; 4],
    radius: f32,
    rgb: u32,
    alpha: u8,
    rings: u32,
) {
    for i in (1..=rings).rev() {
        let d = i as f32 * 2.0;
        ui.rounded(
            x - d,
            y - d,
            w + 2.0 * d,
            h + 2.0 * d,
            radius + d,
            hex(rgb, alpha),
        );
    }
}

/// The plate the pages' content sits on.
pub(super) fn plate(ui: &mut Ui, [x, y, w, h]: [f32; 4]) {
    ui.rounded(x, y, w, h, 14.0, color::CARD);
    ui.border(x, y, w, h, 14.0, color::STROKE);
}

/// A dialog's panel (the account, the exit question).
pub(super) fn panel(ui: &mut Ui, [x, y, w, h]: [f32; 4]) {
    glow(ui, [x, y, w, h], 14.0, 0x6a3cd8, 8, 4);
    ui.rounded(x, y, w, h, 14.0, color::PANEL);
    ui.border(x, y, w, h, 14.0, color::STROKE);
}

/// A row on a plate.
pub(super) fn row(ui: &mut Ui, [x, y, w, h]: [f32; 4]) {
    ui.rounded(x, y, w, h, 8.0, color::ROW);
    ui.border(x, y, w, h, 8.0, color::STROKE);
}

/// A rule across the whole screen.
pub(super) fn rule(ui: &mut Ui, y: f32) {
    let w = ui.w;
    ui.rounded(0.0, y, w, 1.0, 0.0, hex(0x9aa0ff, 26));
}

/// Pixel art: rows of characters, each a texel in the palette's colour (others are clear).
pub(super) struct Sprite {
    pub rows: &'static [&'static str],
    pub palette: &'static [(u8, u32)],
}
impl Sprite {
    pub fn size(&self, texel: f32) -> [f32; 2] {
        [
            self.rows.first().map_or(0, |r| r.len()) as f32 * texel,
            self.rows.len() as f32 * texel,
        ]
    }
    /// Runs of one colour along a row are one quad.
    pub fn draw(&self, ui: &mut Ui, x: f32, y: f32, texel: f32) {
        for (r, row) in self.rows.iter().enumerate() {
            let row = row.as_bytes();
            let mut c = 0;
            while c < row.len() {
                let mut end = c + 1;
                while end < row.len() && row[end] == row[c] {
                    end += 1;
                }
                if let Some((_, colour)) = self.palette.iter().find(|(k, _)| *k == row[c]) {
                    ui.rounded(
                        x + c as f32 * texel,
                        y + r as f32 * texel,
                        (end - c) as f32 * texel,
                        texel,
                        0.0,
                        *colour,
                    );
                }
                c = end;
            }
        }
    }
}

/// The launcher's mark: a violet block, lit from the top left.
pub(super) const CUBE: Sprite = Sprite {
    rows: &[
        ".........oooo.........",
        ".......ooTTTToo.......",
        ".....ooTTTttTTToo.....",
        "...ooTTTttttttTTToo...",
        ".ooTTTttttkkttttTTToo.",
        "oTTTttttkkggkkttttTTTo",
        "oLLTttttkkggkkttttTRRo",
        "oLLLLTttttkkttttTRRRRo",
        "oLlLLLLTttttttTRRRRRro",
        "olLLLLLLLTttTRRRRrRRRo",
        "oLLLhLLLLLeRRRRRRhRrRo",
        "oLLlhhhlLLeRRRRhhhRRRo",
        "oLlLhhhhLLeRRrhhhhrRRo",
        "olLLhhhhLleRRRhhhhRRro",
        "oLLLhhhhlLeRrRhhhhRRRo",
        "oLLlLhhhLLeRRRhhhRRRRo",
        ".ooLLLlhLLeRRRhRrRRoo.",
        "...ooLLLLleRRrRRRoo...",
        ".....ooLLLeRRRRoo.....",
        ".......ooLeRRoo.......",
        ".........oooo.........",
    ],
    palette: &[
        (b'o', hex(0x12092a, 255)),
        (b'T', hex(0xd8beff, 255)),
        (b't', hex(0xae88f8, 255)),
        (b'k', hex(0x5e38b8, 255)),
        (b'g', hex(0xc4a4ff, 255)),
        (b'L', hex(0x885ce2, 255)),
        (b'l', hex(0x744acc, 255)),
        (b'R', hex(0x5836aa, 255)),
        (b'r', hex(0x46288e, 255)),
        (b'h', hex(0x221246, 255)),
        (b'e', hex(0xc09cff, 255)),
    ],
};
/// The account.
pub(super) const PERSON: Sprite = Sprite {
    rows: &[
        "...######...",
        "..########..",
        "..########..",
        "..########..",
        "..########..",
        "...######...",
        "............",
        "...######...",
        ".##########.",
        "############",
        "############",
        "############",
    ],
    palette: &[(b'#', color::TEXT)],
};
/// Play, lit on its upper half.
pub(super) const PLAY: Sprite = Sprite {
    rows: &[
        "P........",
        "PPP......",
        "PPPPP....",
        "PPPPPPP..",
        "PPPPPPPPP",
        "ppppppp..",
        "ppppp....",
        "ppp......",
        "p........",
    ],
    palette: &[(b'P', hex(0xe2c8ff, 255)), (b'p', hex(0xbc94f8, 255))],
};
/// GitHub's mark.
pub(super) const GITHUB: Sprite = Sprite {
    rows: &[
        ".....######.....",
        "...##########...",
        "..############..",
        ".###.######.###.",
        ".###........###.",
        "####........####",
        "###..........###",
        "###..........###",
        "###..........###",
        "###..........###",
        "####........####",
        ".#####....#####.",
        ".##.##....#####.",
        "..##......####..",
        "...###....###...",
        ".....#....#.....",
    ],
    palette: &[(b'#', color::TEXT)],
};

/// How a button looks.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Look {
    /// The violet action, haloed when large.
    Primary,
    /// A dark plate under a fine outline.
    Secondary,
    /// Text being typed: its label sits at the left, where the caret follows it.
    Field,
    /// The account: a dark plate ringed in glowing violet.
    Ring,
}
impl Look {
    /// The face its label is set in (the caret measures typed text with it).
    pub fn face(self) -> Face {
        if self == Look::Field {
            Face::Medium
        } else {
            Face::SemiBold
        }
    }
}

/// The open page's tab: corners cut in two steps at the top, standing on a base a step wider.
fn tab(ui: &mut Ui, [x, y, w, h]: [f32; 4]) {
    let s = 3.0;
    ui.rounded(x + 2.0 * s, y, w - 4.0 * s, s, 0.0, hex(0x7a4ad8, 255));
    ui.rounded(x + s, y + s, w - 2.0 * s, s, 0.0, hex(0x5a2cb4, 255));
    ui.gradient(
        x,
        y + 2.0 * s,
        w,
        h - 3.0 * s,
        hex(0x4c2298, 255),
        hex(0x3a1a7c, 255),
    );
    ui.rounded(x - s, y + h - s, w + 2.0 * s, s, 0.0, hex(0x2f1567, 255));
}

/// The pages' tabs in a row under the header, every name in one size; returns their rects.
pub(super) fn tabs(ui: &mut Ui, labels: &[&str], open: usize) -> Vec<[f32; 4]> {
    let w = ui.w;
    let margin = if w < 700.0 { 16.0 } else { 32.0 };
    let width = (w - 2.0 * margin) / labels.len().max(1) as f32;
    let room = width - 12.0;
    let size = labels
        .iter()
        .map(|label| 16.0 * room / ui.measure(Face::SemiBold, 16.0, label).max(1.0))
        .fold(16.0f32, f32::min)
        .max(9.0);
    let line = (size * 1.45).round();
    let mut rects = Vec::with_capacity(labels.len());
    for (i, label) in labels.iter().enumerate() {
        let rect = [margin + i as f32 * width, 88.0, width - 4.0, 42.0];
        if i == open {
            tab(ui, rect);
        }
        let label_w = ui.measure(Face::SemiBold, size, label);
        ui.text(
            Face::SemiBold,
            size,
            rect[0] + (rect[2] - label_w) / 2.0,
            rect[1] + (rect[3] - line) / 2.0,
            if i == open { color::TEXT } else { color::MUTED },
            label,
        );
        rects.push(rect);
    }
    rule(ui, 131.0);
    rects
}

/// A button's plate and its label (and icon, before the label), centred unless the button is
/// wide or a field.
pub(super) fn button(ui: &mut Ui, rect: [f32; 4], label: &str, look: Look, icon: Option<&Sprite>) {
    let [x, y, w, h] = rect;
    match look {
        Look::Primary => {
            let s = if h >= 50.0 { 3.0 } else { 2.0 };
            if h >= 36.0 {
                glow(ui, rect, 6.0, 0x8a45ff, if h >= 50.0 { 16 } else { 9 }, 5);
            }
            let edge = hex(0xa95ff0, 255);
            stepped(ui, rect, s, 1, edge, edge);
            stepped(
                ui,
                [x + 1.5, y + 1.5, w - 3.0, h - 3.0],
                s,
                1,
                hex(0x7c34dc, 255),
                hex(0x4e1ea6, 255),
            );
        }
        Look::Secondary => {
            ui.rounded(x, y, w, h, 8.0, color::RAISED);
            ui.border(x, y, w, h, 8.0, color::STROKE);
        }
        Look::Field => {
            ui.rounded(x, y, w, h, 6.0, hex(0x05061a, 235));
            ui.border(x, y, w, h, 6.0, hex(0x9aa0ff, 64));
        }
        Look::Ring => {
            glow(ui, rect, 10.0, 0x8a45ff, 10, 5);
            ui.rounded(x, y, w, h, 10.0, hex(0xa06ee6, 255));
            ui.rounded(x + 2.0, y + 2.0, w - 4.0, h - 4.0, 8.0, hex(0x0b0d1f, 255));
        }
    }
    let face = look.face();
    let base = if h >= 56.0 {
        22.0
    } else if w < 80.0 {
        13.0
    } else {
        16.0
    };
    let padding = (w * 0.12).clamp(4.0, 12.0);
    let texel = ((h / 26.0).clamp(1.25, 2.5) * 4.0).round() / 4.0;
    let icon_w = icon.map_or(0.0, |i| i.size(texel)[0] + base * 0.6);
    let room = w - 2.0 * padding - icon_w;
    let font = base;
    let label = ui.fit_text(face, font, label, room);
    let width = ui.measure(face, font, &label);
    let left = if look == Look::Field || (w > 360.0 && icon.is_none()) {
        x + padding
    } else {
        x + ((w - icon_w - width) / 2.0).max(padding)
    };
    if let Some(icon) = icon {
        let [_, ih] = icon.size(texel);
        icon.draw(ui, left, y + ((h - ih) / 2.0).round(), texel);
    }
    let line = (font * 1.45).round();
    ui.text(
        face,
        font,
        left + icon_w,
        y + (h - line) / 2.0,
        if look == Look::Primary {
            color::ON_ACCENT
        } else {
            color::TEXT
        },
        &label,
    );
}

/// The top of every page: the cube and the wordmark at the left, the version after it, the
/// account at the right (under them on phones held upright), and a rule below; returns the
/// account's rect.
pub(super) fn header(ui: &mut Ui, version: &str, account: &str) -> [f32; 4] {
    let w = ui.w;
    let compact = w < 700.0;
    let (texel, x, y) = if compact {
        (1.5, 16.0, 7.0)
    } else {
        (2.5, 30.0, 14.0)
    };
    CUBE.draw(ui, x, y, texel);
    let [cube_w, cube_h] = CUBE.size(texel);
    let size: f32 = if w < 400.0 {
        20.0
    } else if compact {
        24.0
    } else if w < 1000.0 {
        34.0
    } else {
        40.0
    };
    let line = (size * 1.45).round();
    let left = x + cube_w + if compact { 10.0 } else { 16.0 };
    let top = y + (cube_h - line) / 2.0;
    let shadow = hex(0x1a0b3c, 255);
    let drop = (size / 14.0).round();
    ui.text(
        Face::Pixel,
        size,
        left + drop,
        top + drop,
        shadow,
        "Horizon",
    );
    ui.text(Face::Pixel, size, left, top, color::TEXT, "Horizon");
    let after = left + ui.measure(Face::Pixel, size, "Horizon ");
    ui.text(
        Face::Pixel,
        size,
        after + drop,
        top + drop,
        shadow,
        "Android",
    );
    let end = after + ui.text(Face::Pixel, size, after, top, color::ACCENT_HI, "Android");
    let small = if compact { 12.0 } else { 14.0 };
    ui.text(
        Face::Medium,
        small,
        end + drop + 10.0,
        top + line * 0.62 - (small * 1.45).round() * 0.5,
        color::MUTED,
        version,
    );
    let rect = if compact {
        [16.0, 48.0, w - 32.0, 34.0]
    } else {
        let width = (ui.measure(Face::SemiBold, 16.0, account) + 92.0).clamp(170.0, 320.0);
        [w - 32.0 - width, 17.0, width, 50.0]
    };
    button(ui, rect, account, Look::Ring, Some(&PERSON));
    rule(ui, if compact { 85.0 } else { 80.0 });
    rect
}

/// A small deterministic generator, so the night is the same every frame.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The night behind every page: a dark sky with stars, far hills, a castle with lit windows in
/// violet fog at the right, and blocky ground in front. It stands still (no frames are asked for).
pub(super) fn backdrop(ui: &mut Ui) {
    let (w, h) = (ui.w, ui.h);
    // Texels of the pixel art, as large as the screen is tall.
    let t = (h / 140.0).clamp(3.0, 6.0).round();
    let snap = |v: f32| (v / t).round() * t;
    let horizon = snap(h * 0.62);
    ui.gradient(0.0, 0.0, w, horizon, hex(0x02040e, 255), hex(0x120d36, 255));
    ui.gradient(
        0.0,
        horizon,
        w,
        h - horizon,
        hex(0x120d36, 255),
        hex(0x05040f, 255),
    );
    let mut rng = Rng(0x2545_f491);
    let stars = (w * h / 12000.0) as usize;
    for _ in 0..stars {
        let x = snap(rng.next() * w);
        let y = snap(rng.next().powf(1.4) * horizon * 0.85);
        let big = rng.next() < 0.1;
        // Fainter behind the header and the tabs.
        let alpha = ((30.0 + rng.next() * 150.0) * if y < 136.0 { 0.5 } else { 1.0 }) as u8;
        let s = if big { t } else { (t * 0.5).round().max(1.0) };
        ui.rounded(x, y, s, s, 0.0, hex(0xe9e4ff, alpha));
        if big {
            let a = alpha / 3;
            ui.rounded(x - s, y, s, s, 0.0, hex(0xe9e4ff, a));
            ui.rounded(x + s, y, s, s, 0.0, hex(0xe9e4ff, a));
            ui.rounded(x, y - s, s, s, 0.0, hex(0xe9e4ff, a));
            ui.rounded(x, y + s, s, s, 0.0, hex(0xe9e4ff, a));
        }
    }
    // The castle stands at the right, where the Play page leaves the night uncovered.
    let castle = snap(if w < 700.0 { w * 0.72 } else { w * 0.83 });
    // Its glow in the fog.
    for i in 0..12 {
        let r = (12 - i) as f32 * h * 0.03;
        ui.rounded(
            castle - r * 2.0,
            horizon - r * 0.8,
            r * 4.0,
            r * 1.6,
            r * 0.8,
            hex(0x5a2ab8, 6),
        );
    }
    // Far hills: two slow waves and a quick one, cut into texel columns.
    let ridge = |x: f32, base: f32, amp: f32, phase: f32| {
        base - amp
            * (0.55 * (x * 0.0042 + phase).sin()
                + 0.3 * (x * 0.0113 + phase * 2.0).sin()
                + 0.15 * (x * 0.031 + phase * 3.0).sin())
    };
    let column = 2.0 * t;
    let columns = (w / column).ceil() as usize + 1;
    for i in 0..columns {
        let x = i as f32 * column;
        let y = snap(ridge(x, horizon - t * 5.0, h * 0.045, 0.7));
        ui.rounded(x, y, column, h - y, 0.0, hex(0x0d0b28, 255));
    }
    // Banks of fog over the hills, thicker towards the castle.
    let mut fog = |ui: &mut Ui, y: f32, strength: f32| {
        let mut x = -rng.next() * 30.0 * t;
        while x < w {
            let len = snap((14.0 + rng.next() * 40.0) * t);
            let near = 1.0 - ((x + len * 0.5 - castle).abs() / (w * 0.5)).min(1.0);
            let alpha = ((6.0 + 30.0 * near * near) * strength) as u8;
            ui.rounded(x, y, len, 2.0 * t, 0.0, hex(0x7040d0, alpha));
            ui.rounded(
                x + 3.0 * t,
                y - t,
                (len - 6.0 * t).max(0.0),
                t,
                0.0,
                hex(0x7040d0, alpha / 2),
            );
            x += len + snap((10.0 + rng.next() * 30.0) * t);
        }
    };
    for i in 0..3 {
        fog(ui, snap(horizon - t * 6.0 + i as f32 * t * 5.0), 1.0);
    }
    castle_silhouette(ui, castle, horizon, t);
    for i in 0..3 {
        fog(ui, snap(horizon + t * 2.0 + i as f32 * t * 6.0), 0.8);
    }
    // Nearer hills with blocky pines.
    for i in 0..columns {
        let x = i as f32 * column;
        let y = snap(ridge(x, h * 0.83, h * 0.06, 2.9));
        ui.rounded(x, y, column, h - y, 0.0, hex(0x07061a, 255));
        if rng.next() < 0.22 {
            let tall = (3.0 + rng.next() * 6.0).round();
            for k in 0..tall as usize {
                let k = k as f32;
                let half = ((tall - k) * 0.5).ceil().min(3.0);
                ui.rounded(
                    x + t - half * t,
                    y - (k + 1.0) * t,
                    half * 2.0 * t,
                    t,
                    0.0,
                    hex(0x07061a, 255),
                );
            }
        }
    }
    // Ground: blocks in front, their tops catching a little light.
    let block = 3.0 * t;
    for i in 0..(w / block).ceil() as usize + 1 {
        let x = i as f32 * block;
        let y = snap(ridge(x, h * 0.93, h * 0.035, 5.3));
        ui.rounded(x, y, block, h - y, 0.0, hex(0x030308, 255));
        ui.rounded(x, y, block, t, 0.0, hex(0x1a1436, 200));
    }
    // A last mist low at the right.
    for i in 0..4u8 {
        let y = snap(h * 0.86 + f32::from(i) * t * 2.0);
        ui.rounded(
            castle - w * 0.3,
            y,
            w * 0.5,
            t * 2.0,
            0.0,
            hex(0x6a3cc4, 12 - i * 2),
        );
    }
}

/// A castle on the horizon at `x`: a keep between towers on one wall, spires cut in steps, a
/// few windows lit violet.
fn castle_silhouette(ui: &mut Ui, x: f32, horizon: f32, t: f32) {
    let stone = hex(0x0e0a28, 255);
    let edge = hex(0x1b1446, 255);
    // (offset, width, height, spire) in texels.
    const TOWERS: [(f32, f32, f32, f32); 7] = [
        (-15.0, 4.0, 17.0, 5.0),
        (12.0, 4.0, 19.0, 5.0),
        (-10.0, 5.0, 24.0, 6.0),
        (6.0, 5.0, 26.0, 7.0),
        (-4.0, 9.0, 33.0, 9.0),
        (-19.0, 3.0, 11.0, 4.0),
        (17.0, 3.0, 12.0, 4.0),
    ];
    let base = horizon + t * 3.0;
    // The wall the towers stand on, crenellated.
    ui.rounded(
        x - 19.0 * t,
        base - 10.0 * t,
        39.0 * t,
        10.0 * t,
        0.0,
        stone,
    );
    for k in 0..20 {
        let cx = x - 19.0 * t + k as f32 * 2.0 * t;
        ui.rounded(cx, base - 11.0 * t, t, t, 0.0, stone);
    }
    for (offset, width, height, spire) in TOWERS {
        let left = x + offset * t;
        let top = base - height * t;
        ui.rounded(left, top, width * t, height * t, 0.0, stone);
        // The edge facing the glow.
        ui.rounded(left, top, t * 0.5, height * t - 10.0 * t, 0.0, edge);
        for k in 0..spire as usize {
            let inset = (k as f32 * width * 0.5 / spire).floor() * t;
            let span = (width * t - 2.0 * inset).max(t);
            ui.rounded(
                left + (width * t - span) / 2.0,
                top - (k + 1) as f32 * t,
                span,
                t,
                0.0,
                stone,
            );
        }
        // Lit slits, one per eight texels of tower above the wall.
        let slits = ((height - 10.0) / 8.0) as usize;
        let wx = left + (width * t - t * 0.5) / 2.0;
        for k in 0..slits {
            let wy = top + (3.0 + k as f32 * 8.0) * t;
            ui.rounded(wx - t, wy - t, t * 2.5, t * 3.0, 0.0, hex(0x8a4fff, 28));
            ui.rounded(wx, wy, t * 0.5, t, 0.0, hex(0xc9a8ff, 235));
        }
    }
    // The keep's great window.
    let gx = x - 0.5 * t;
    let gy = base - 21.0 * t;
    ui.rounded(gx - t, gy - t, 3.0 * t, 5.0 * t, 0.0, hex(0x8a4fff, 40));
    ui.rounded(gx, gy, t, 3.0 * t, 0.0, hex(0xb98cff, 220));
}

/// A numbered step's square: violet when it is the one to do, dim once done, an outline ahead.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Badge {
    Lit,
    Done,
    Ahead,
}
pub(super) fn badge(ui: &mut Ui, [x, y, w, h]: [f32; 4], number: &str, state: Badge) {
    let s = 2.0;
    match state {
        Badge::Lit => {
            let edge = hex(0xa070e8, 255);
            stepped(ui, [x, y, w, h], s, 1, edge, edge);
            stepped(
                ui,
                [x + 1.5, y + 1.5, w - 3.0, h - 3.0],
                s,
                1,
                hex(0x864ed0, 255),
                hex(0x572a9e, 255),
            );
        }
        Badge::Done => {
            let fill = hex(0x2c1a5c, 255);
            stepped(ui, [x, y, w, h], s, 1, fill, fill);
        }
        Badge::Ahead => {
            let edge = hex(0x9aa0ff, 60);
            stepped(ui, [x, y, w, h], s, 1, edge, edge);
            let fill = hex(0x0a0c21, 255);
            stepped(ui, [x + 1.5, y + 1.5, w - 3.0, h - 3.0], s, 1, fill, fill);
        }
    }
    let size = (h * 0.5).round();
    let width = ui.measure(Face::Pixel, size, number);
    let line = (size * 1.45).round();
    ui.text(
        Face::Pixel,
        size,
        x + (w - width) / 2.0,
        y + (h - line) / 2.0,
        if state == Badge::Lit {
            color::ON_ACCENT
        } else {
            color::MUTED
        },
        number,
    );
}

/// What the Play page's plate says under its title.
pub(super) enum PlayBody<'a> {
    /// Three numbered steps, then the main action and GitHub.
    Steps {
        steps: [&'a str; 3],
        action: &'a str,
    },
    /// The install's stages (the current one lit), its progress and the detail under it.
    Installing {
        stages: &'a [&'a str],
        current: usize,
        overall: f32,
        detail: &'a str,
    },
}

/// The Play page: one plate under the tabs (the left of a landscape screen, the night beside it)
/// with a pixel title, a subtitle, the body, and the main action beside GitHub. The targets are
/// generic so the layout can be drawn outside the launcher (previews).
pub(super) fn play_card<A: Copy>(
    ui: &mut Ui,
    targets: &mut Vec<([f32; 4], A)>,
    title: &str,
    subtitle: &str,
    body: &PlayBody,
    play: A,
    repository: A,
) {
    let (w, h) = (ui.w, ui.h);
    let compact = w < 700.0;
    let (x, top) = if compact {
        (16.0, 140.0)
    } else {
        (32.0, 146.0)
    };
    let width = if compact {
        w - 32.0
    } else {
        (w - 64.0).min(800.0)
    };
    let pad = if compact { 20.0 } else { 34.0 };
    // Above the status line.
    let room = h - 60.0 - top;
    let mut title_size: f32 = if compact { 30.0 } else { 46.0 };
    let sub_size: f32 = if compact { 15.0 } else { 18.0 };
    let button_h: f32 = if compact { 56.0 } else { 64.0 };
    let rows = match body {
        PlayBody::Steps { .. } => 3.0,
        PlayBody::Installing { stages, .. } => stages.len() as f32,
    };
    let after_rows = match body {
        PlayBody::Steps { .. } => 22.0 + button_h,
        PlayBody::Installing { .. } => 56.0,
    };
    let natural_row: f32 = match body {
        PlayBody::Steps { .. } if compact => 48.0,
        PlayBody::Steps { .. } => 54.0,
        PlayBody::Installing { .. } => 40.0,
    };
    let fixed = |title_size: f32| {
        pad * 0.7
            + (title_size * 1.45).round()
            + (sub_size * 1.45).round()
            + 22.0
            + after_rows
            + pad
    };
    if fixed(title_size) + rows * natural_row > room {
        title_size = if compact { 24.0 } else { 34.0 };
    }
    let row_h = ((room - fixed(title_size)) / rows).clamp(26.0, natural_row);
    // Too short for every stage: the current one alone.
    let list = rows * 26.0 <= room - fixed(title_size);
    let badge_h = match body {
        PlayBody::Steps { .. } => (row_h - 14.0).clamp(26.0, 40.0),
        PlayBody::Installing { .. } => (row_h - 10.0).clamp(20.0, 30.0),
    };
    // The last row ends with its badge.
    let rows_h = if list {
        rows * row_h - (row_h - badge_h)
    } else {
        70.0
    };
    let height = fixed(title_size) + rows_h;
    plate(ui, [x, top, width, height]);
    let inner = x + pad;
    let inner_w = width - 2.0 * pad;
    let mut y = top + pad * 0.7;
    let title = ui.fit_text(Face::Pixel, title_size, title, inner_w);
    ui.text(
        Face::Pixel,
        title_size,
        inner + 2.0,
        y + 3.0,
        hex(0x1a0b3c, 255),
        &title,
    );
    ui.text(Face::Pixel, title_size, inner, y, color::TEXT, &title);
    y += (title_size * 1.45).round();
    ui.text(Face::Regular, sub_size, inner, y, color::MUTED, subtitle);
    y += (sub_size * 1.45).round() + 22.0;
    let text_size = if compact { 16.0 } else { 19.0 };
    match body {
        PlayBody::Steps { steps, action } => {
            let badge_w = (badge_h * 1.22).round();
            for (i, step) in steps.iter().enumerate() {
                let ry = y + i as f32 * row_h;
                badge(
                    ui,
                    [inner, ry, badge_w, badge_h],
                    &format!("{:02}", i + 1),
                    Badge::Lit,
                );
                let bar = inner + badge_w + 14.0;
                ui.rounded(bar, ry + 6.0, 1.0, badge_h - 12.0, 0.0, color::STROKE);
                let text_x = bar + 16.0;
                let step = ui.fit_text(Face::Regular, text_size, step, inner + inner_w - text_x);
                ui.text(
                    Face::Regular,
                    text_size,
                    text_x,
                    ry + (badge_h - (text_size * 1.45).round()) / 2.0,
                    color::SOFT,
                    &step,
                );
            }
            let by = y + rows_h + 22.0;
            let gap = 16.0;
            let play_w = if compact {
                ((inner_w - gap) * 0.6).round()
            } else {
                (inner_w * 0.46).clamp(220.0, 320.0)
            };
            let repo_w = if compact {
                inner_w - gap - play_w
            } else {
                (inner_w * 0.28).clamp(150.0, 200.0)
            };
            let play_rect = [inner, by, play_w, button_h];
            button(ui, play_rect, action, Look::Primary, Some(&PLAY));
            targets.push((play_rect, play));
            let repo_rect = [inner + play_w + gap, by + 3.0, repo_w, button_h - 6.0];
            button(ui, repo_rect, "GitHub", Look::Secondary, Some(&GITHUB));
            targets.push((repo_rect, repository));
        }
        PlayBody::Installing {
            stages,
            current,
            overall,
            detail,
        } => {
            if list {
                let badge_w = (badge_h * 1.22).round();
                for (i, stage) in stages.iter().enumerate() {
                    let ry = y + i as f32 * row_h;
                    let state = match i.cmp(current) {
                        std::cmp::Ordering::Less => Badge::Done,
                        std::cmp::Ordering::Equal => Badge::Lit,
                        std::cmp::Ordering::Greater => Badge::Ahead,
                    };
                    badge(
                        ui,
                        [inner, ry, badge_w, badge_h],
                        &format!("{:02}", i + 1),
                        state,
                    );
                    let size = text_size - 2.0;
                    let text_y = ry + (badge_h - (size * 1.45).round()) / 2.0;
                    let end = ui.text(
                        Face::Regular,
                        size,
                        inner + badge_w + 16.0,
                        text_y,
                        if state == Badge::Lit {
                            color::TEXT
                        } else {
                            color::MUTED
                        },
                        stage,
                    );
                    if state == Badge::Done {
                        ui.text(
                            Face::Medium,
                            size - 2.0,
                            inner + badge_w + 28.0 + end,
                            text_y + 1.0,
                            color::SUCCESS,
                            "done",
                        );
                    }
                }
            } else {
                ui.text(
                    Face::Medium,
                    text_size,
                    inner,
                    y,
                    color::TEXT,
                    stages.get(*current).copied().unwrap_or_default(),
                );
                ui.text(
                    Face::Regular,
                    15.0,
                    inner,
                    y + 34.0,
                    color::MUTED,
                    &format!("Step {} of {}", current + 1, stages.len()),
                );
            }
            let bar_y = y + rows_h + 16.0;
            progress(ui, [inner, bar_y, inner_w, 10.0], *overall);
            let detail = ui.fit_text(Face::Regular, 14.0, detail, inner_w);
            ui.text(
                Face::Regular,
                14.0,
                inner,
                bar_y + 18.0,
                color::MUTED,
                &detail,
            );
        }
    }
}

/// A pixel progress bar.
pub(super) fn progress(ui: &mut Ui, [x, y, w, h]: [f32; 4], fraction: f32) {
    ui.rounded(x, y, w, h, 0.0, hex(0x1b1838, 255));
    let filled = (w * fraction.clamp(0.0, 1.0)).round();
    if filled > 0.0 {
        ui.gradient(x, y, filled, h, hex(0x9a5cf4, 255), hex(0x5a26b4, 255));
        ui.rounded(x, y, filled, 2.0, 0.0, hex(0xc8a4ff, 255));
    }
}
