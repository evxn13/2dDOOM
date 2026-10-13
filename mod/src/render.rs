//! Rendu vue du dessus, directement dans le tampon 8 bits de Doom.

use doomwad::Patch;

use crate::assets::{Assets, fogged, light_colormap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::floorcache::{self, CELL_SHIFT, FloorCache, VOID};
use crate::game::*;
use crate::level::Level;
use crate::visibility::Visibility;

/// Indices dans la palette PLAYPAL de Doom.
mod color {
    pub const BACKGROUND: u8 = 0; // noir
    pub const ROOF: u8 = 109; // gris sombre : hors de la carte (« toits »)
    pub const WALL: u8 = 80; // gris clair
    pub const LEDGE: u8 = 104; // gris moyen : rebord infranchissable (> 24 unités)
    // Lignes activables (« utiliser ») :
    pub const DOOR: u8 = 216; // orange : porte
    pub const BLUE_KEY: u8 = 200;
    pub const YELLOW_KEY: u8 = 231;
    pub const RED_KEY: u8 = 176;
    pub const SWITCH: u8 = 112; // vert : interrupteur
    pub const EXIT: u8 = 4; // blanc : sortie du niveau
    pub const CROSSHAIR: u8 = 4; // blanc
}

/// Les pixels de Doom (320x200 affiché en 4:3) sont 1,2 fois plus hauts que larges.
pub const PIXEL_ASPECT: f32 = 1.2;

/// Zone de l'écran où le jeu dessinait sa vue 3D.
pub struct Canvas {
    pub buf: *mut u8,
    pub stride: usize,
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
}

impl Canvas {
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            let off = (y + self.y0) as usize * self.stride + (x + self.x0) as usize;
            unsafe { *self.buf.add(off) = c };
        }
    }

    /// Ligne `y` de la vue (y dans [0, h)).
    pub fn row(&mut self, y: i32) -> &mut [u8] {
        assert!(y >= 0 && y < self.h);
        let off = (y + self.y0) as usize * self.stride + self.x0 as usize;
        unsafe { core::slice::from_raw_parts_mut(self.buf.add(off), self.w as usize) }
    }

    pub fn clear(&mut self, c: u8) {
        for y in 0..self.h {
            let off = (y + self.y0) as usize * self.stride + self.x0 as usize;
            unsafe { core::ptr::write_bytes(self.buf.add(off), c, self.w as usize) };
        }
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: u8) {
        let Some(((x0, y0), (x1, y1))) = clip(a, b, (self.w - 1) as f32, (self.h - 1) as f32) else {
            return;
        };
        // Bresenham.
        let (mut x, mut y) = (x0.round() as i32, y0.round() as i32);
        let (x1, y1) = (x1.round() as i32, y1.round() as i32);
        let dx = (x1 - x).abs();
        let dy = -(y1 - y).abs();
        let sx = if x < x1 { 1 } else { -1 };
        let sy = if y < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
}

/// Découpage de segment de Liang–Barsky sur [0, xmax] x [0, ymax].
fn clip(a: (f32, f32), b: (f32, f32), xmax: f32, ymax: f32) -> Option<((f32, f32), (f32, f32))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, a.0), (dx, xmax - a.0), (-dy, a.1), (dy, ymax - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some(((a.0 + t0 * dx, a.1 + t0 * dy), (a.0 + t1 * dx, a.1 + t1 * dy)))
}

/// Caméra fixe orientée nord, centrée sur le joueur (comme GTA 1/2).
struct Camera {
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    /// Pixels par unité de carte.
    scale: f32,
}

const FRAC: f32 = (1 << FRACBITS) as f32;

impl Camera {
    fn project(&self, x: i32, y: i32) -> (f32, f32) {
        (
            self.cx + (x as f32 / FRAC - self.px) * self.scale,
            self.cy - (y as f32 / FRAC - self.py) * self.scale / PIXEL_ASPECT,
        )
    }
}

/// Tout ce qu'il faut pour dessiner une image.
pub struct Frame<'a> {
    pub player: &'a Player,
    pub level: Level<'a>,
    pub sectors: &'a [Sector],
    pub assets: &'a Assets,
    pub flattranslation: *const i32,
    pub skyflatnum: i32,
    pub sprites: &'a [SpriteDef],
    pub units_per_pixel: f32,
    pub wall_height: f32,
    pub fog: bool,
    pub timing: bool,
}

/// Différence de hauteur au-delà de laquelle le joueur ne peut plus monter.
const MAX_STEP: i32 = 24 << FRACBITS;

pub fn draw_frame(canvas: &mut Canvas, f: &Frame) {
    if f.player.mo.is_null() {
        canvas.clear(color::BACKGROUND);
        return;
    }
    let mo = unsafe { &*f.player.mo };
    let cam = Camera {
        px: mo.x as f32 / FRAC,
        py: mo.y as f32 / FRAC,
        cx: canvas.w as f32 / 2.0,
        cy: canvas.h as f32 / 2.0,
        scale: 1.0 / f.units_per_pixel,
    };
    let t0 = Instant::now();
    // Rayon couvert par l'écran, en unités de carte.
    let radius = (canvas.w.max(canvas.h) as f32) / cam.scale * PIXEL_ASPECT;
    let vis = f.fog.then(|| Visibility::compute(f.level.lines, cam.px, cam.py, radius));
    draw_floors(canvas, f, &cam, vis.as_ref());
    let t1 = Instant::now();
    draw_lines(canvas, f, &cam);
    let t2 = Instant::now();
    let mut items = collect_faces(canvas, f, &cam, vis.as_ref());
    collect_sprites(&mut items, canvas, f, &cam, vis.as_ref());
    // Du haut vers le bas de l'écran : ce qui est plus près de la caméra passe devant.
    items.sort_by(|a, b| a.key().total_cmp(&b.key()));
    for item in &items {
        match item {
            Item::Sprite(v) => draw_sprite(canvas, f.assets, v, cam.scale),
            Item::Face(face) => draw_face(canvas, f, face, cam.scale, vis.as_ref()),
        }
    }
    draw_usable_lines(canvas, f, &cam);
    let t3 = Instant::now();
    if f.timing {
        profile([t1 - t0, t2 - t1, t3 - t2]);
    }
}

/// Moyennes par image, affichées toutes les 500 images.
fn profile(t: [Duration; 3]) {
    static ACC: Mutex<([Duration; 3], u32)> = Mutex::new(([Duration::ZERO; 3], 0));
    let mut acc = ACC.lock().unwrap_or_else(|e| e.into_inner());
    for (a, d) in acc.0.iter_mut().zip(t) {
        *a += d;
    }
    acc.1 += 1;
    if acc.1 == 500 {
        let us = |d: Duration| d.as_micros() / 500;
        eprintln!(
            "[topdoom] µs/image : sols {} · contours {} · murs+sprites {}",
            us(acc.0[0]), us(acc.0[1]), us(acc.0[2])
        );
        *acc = ([Duration::ZERO; 3], 0);
    }
}

static FLOOR_CACHE: Mutex<Option<FloorCache>> = Mutex::new(None);

fn draw_floors(canvas: &mut Canvas, f: &Frame, cam: &Camera, vis: Option<&Visibility>) {
    let mut guard = FLOOR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(FloorCache::new);
    cache.sync(&f.level);
    let (extra, fixed) = (f.player.extralight, f.player.fixedcolormap);
    // Virgule fixe 16.16 : `>> 16` arrondit vers le bas sans appel à floor().
    let step = (FRAC / cam.scale) as i32;
    let wx0 = ((cam.px + (0.5 - cam.cx) / cam.scale) * FRAC) as i32;
    for y in 0..canvas.h {
        let wy = ((cam.py - (y as f32 + 0.5 - cam.cy) * PIXEL_ASPECT / cam.scale) * FRAC) as i32;
        let iy = wy >> FRACBITS;
        let cell_y = iy >> CELL_SHIFT;
        let v = ((-iy - 1) & 63) as usize; // ligne du flat : floor(-y)
        let row = canvas.row(y);
        // Les tuiles du cache sont dans des Box : leur adresse reste stable.
        let mut tile_key = (i32::MIN, i32::MIN);
        let mut cells: *const u16 = core::ptr::null();
        // Ce qu'il faut pour peindre le dernier secteur rencontré.
        let mut shade_sector = u16::MAX - 1;
        let mut shade: Option<(&[u8], &[u8], &[u8])> = None;
        let fy = iy as f32;
        let mut wx = wx0;
        let mut seen = true;
        for (x, px) in row.iter_mut().enumerate() {
            let ix = wx >> FRACBITS;
            let (tile, idx) = floorcache::split(ix >> CELL_SHIFT, cell_y);
            if tile != tile_key {
                tile_key = tile;
                cells = cache.tile(&f.level, f.sectors, tile.0, tile.1).as_ptr();
            }
            let sector = unsafe { *cells.add(idx) };
            if sector != shade_sector {
                shade_sector = sector;
                shade = if sector == VOID { None } else { sector_shade(f, &f.sectors[sector as usize], extra, fixed) };
            }
            *px = match shade {
                Some((flat, lit, dark)) => {
                    // Un test sur deux pixels suffit à l'œil.
                    if x & 1 == 0 {
                        seen = vis.is_none_or(|v| v.sees(ix as f32, fy));
                    }
                    let cmap = if seen { lit } else { dark };
                    cmap[flat[v * 64 + (ix & 63) as usize] as usize]
                }
                None => color::ROOF,
            };
            wx += step;
        }
    }
}

/// Texture (64x64) et tables d'éclairage (en vue / hors de vue) d'un secteur.
fn sector_shade<'a>(f: &Frame<'a>, sector: &Sector, extra: i32, fixed: i32) -> Option<(&'a [u8], &'a [u8], &'a [u8])> {
    // Porte ou passage fermé : on voit le dessus (le plafond du secteur).
    let pic = if sector.ceilingheight <= sector.floorheight { sector.ceilingpic } else { sector.floorpic };
    if pic as i32 == f.skyflatnum || pic < 0 {
        return None;
    }
    let flatnum = unsafe { *f.flattranslation.add(pic as usize) };
    let flat = f.assets.flat(flatnum)?;
    let cmap = light_colormap(sector.lightlevel, extra, fixed);
    Some((flat, f.assets.colormap(cmap), f.assets.colormap(fogged(cmap))))
}

fn line_color(line: &Line) -> Option<u8> {
    if line.backsector.is_null() || line.flags & ML_SECRET != 0 {
        return Some(color::WALL);
    }
    let (front, back) = unsafe { (&*line.frontsector, &*line.backsector) };
    if back.ceilingheight <= back.floorheight || front.ceilingheight <= front.floorheight {
        Some(color::LEDGE)
    } else if (front.floorheight - back.floorheight).abs() > MAX_STEP {
        Some(color::LEDGE)
    } else {
        None // marche franchissable ou simple changement de plafond
    }
}

fn draw_lines(canvas: &mut Canvas, f: &Frame, cam: &Camera) {
    for line in f.level.lines {
        if let Some(c) = line_color(line) {
            let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
            canvas.line(cam.project(v1.x, v1.y), cam.project(v2.x, v2.y), c);
        }
    }
}

/// Angle sous lequel la caméra voit les objets : depuis le sud, vers le nord.
const VIEW_ANGLE: u32 = 0x4000_0000;

struct VisSprite<'a> {
    patch: &'a Patch,
    flip: bool,
    sx: f32,
    /// Ligne des pieds à l'écran (tient compte de l'altitude).
    feet: f32,
    /// Ligne du sol sous l'objet : clé de tri.
    ground: f32,
    cmap: Option<usize>,
    shadow: bool,
}

/// Face de mur tournée vers la caméra, dessinée en relief.
struct Face<'a> {
    tex: &'a doomwad::Texture,
    /// Extrémités de la base à l'écran (x0 < x1).
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    /// Hauteur en pixels.
    height: f32,
    /// Colonne de texture à l'extrémité x0, et longueur en unités.
    u0: f32,
    len: f32,
    /// Extrémités en unités de carte (pour le brouillard).
    a: (f32, f32),
    b: (f32, f32),
    cmap: usize,
}

enum Item<'a> {
    Sprite(VisSprite<'a>),
    Face(Face<'a>),
}

impl Item<'_> {
    fn key(&self) -> f32 {
        match self {
            Item::Sprite(v) => v.ground,
            Item::Face(f) => f.y0.max(f.y1),
        }
    }
}

fn collect_faces<'a>(canvas: &Canvas, f: &Frame<'a>, cam: &Camera, _vis: Option<&Visibility>) -> Vec<Item<'a>> {
    let mut items = Vec::with_capacity(512);
    let (w, h) = (canvas.w as f32, canvas.h as f32);
    let max_h = f.wall_height.max(0.0);
    for ss in f.level.subsectors {
        for i in 0..ss.numlines.max(0) as usize {
            let seg = unsafe { &*f.level.segs.add(ss.firstline as usize + i) };
            let (v1, v2) = unsafe { (&*seg.v1, &*seg.v2) };
            // Visible depuis le sud seulement si le segment va vers l'est
            // (son côté avant, à droite, regarde alors vers le bas de l'écran).
            if v2.x <= v1.x || seg.sidedef.is_null() || seg.frontsector.is_null() {
                continue;
            }
            let (x0, y0) = cam.project(v1.x, v1.y);
            let (x1, y1) = cam.project(v2.x, v2.y);
            if x1 < 0.0 || x0 > w || y0.min(y1) - max_h * cam.scale > h || y0.max(y1) < 0.0 {
                continue;
            }
            let side = unsafe { &*seg.sidedef };
            let front = unsafe { &*seg.frontsector };
            let (tex, units) = match unsafe { seg.backsector.as_ref() } {
                None => (side.midtexture, (front.ceilingheight - front.floorheight) as f32 / FRAC),
                Some(back) if back.ceilingheight <= back.floorheight => (side.toptexture, max_h),
                Some(back) if back.floorheight > front.floorheight => {
                    (side.bottomtexture, (back.floorheight - front.floorheight) as f32 / FRAC)
                }
                _ => continue,
            };
            let height = units.min(max_h) * cam.scale;
            let Some(tex) = f.assets.textures.get(tex as usize).filter(|_| tex > 0 && height >= 1.0) else {
                continue;
            };
            let a = (v1.x as f32 / FRAC, v1.y as f32 / FRAC);
            let b = (v2.x as f32 / FRAC, v2.y as f32 / FRAC);
            items.push(Item::Face(Face {
                tex,
                x0,
                y0,
                x1,
                y1,
                height,
                u0: (seg.offset.wrapping_add(side.textureoffset)) as f32 / FRAC,
                len: ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt(),
                a,
                b,
                cmap: light_colormap(front.lightlevel, f.player.extralight, f.player.fixedcolormap),
            }));
        }
    }
    items
}

fn draw_face(canvas: &mut Canvas, f: &Frame, face: &Face, scale: f32, vis: Option<&Visibility>) {
    let lit = f.assets.colormap(face.cmap);
    let dark = f.assets.colormap(fogged(face.cmap));
    let span = face.x1 - face.x0;
    if span < 0.5 {
        return; // mur vu exactement de profil
    }
    let tex = face.tex;
    for x in face.x0.ceil().max(0.0) as i32..face.x1.ceil().min(canvas.w as f32) as i32 {
        let t = (x as f32 + 0.5 - face.x0) / span;
        let base = face.y0 + t * (face.y1 - face.y0);
        let top = base - face.height;
        let u = (face.u0 + t * face.len) as i32;
        let wx = face.a.0 + t * (face.b.0 - face.a.0);
        let wy = face.a.1 + t * (face.b.1 - face.a.1);
        // Le pied du mur est juste devant lui : on teste un point légèrement au sud.
        let cmap = if vis.is_none_or(|v| v.sees(wx, wy - 2.0)) { lit } else { dark };
        for y in top.ceil().max(0.0) as i32..base.ceil().min(canvas.h as f32) as i32 {
            // On montre le bas de la texture, à l'échelle d'un texel par unité.
            let up = ((base - y as f32 - 0.5) / scale) as i32;
            if let Some(c) = tex.pixel(u, tex.height - 1 - up) {
                canvas.put(x, y, cmap[c as usize]);
            }
        }
    }
}

fn collect_sprites<'a>(items: &mut Vec<Item<'a>>, canvas: &Canvas, f: &Frame<'a>, cam: &Camera, vis: Option<&Visibility>) {
    let margin = 64.0;
    let player_mo = f.player.mo as *const Mobj;
    let mut collect = |m: &Mobj| {
        let (sx, sy) = cam.project(m.x, m.y);
        if sx < -margin || sy < -margin || sx > canvas.w as f32 + margin || sy > canvas.h as f32 + 2.0 * margin {
            return;
        }
        if !core::ptr::eq(m, player_mo) && vis.is_some_and(|v| !v.sees(m.x as f32 / FRAC, m.y as f32 / FRAC)) {
            return; // hors de vue : caché par le brouillard
        }
        let Some(def) = f.sprites.get(m.sprite as usize) else { return };
        let frame = m.frame & FF_FRAMEMASK;
        if def.spriteframes.is_null() || frame >= def.numframes {
            return;
        }
        let sf = unsafe { &*def.spriteframes.add(frame as usize) };
        // Même formule que R_ProjectSprite, avec un point de vue fixe.
        let rot = if sf.rotate == 0 {
            0
        } else {
            (VIEW_ANGLE.wrapping_sub(m.angle).wrapping_add(0x9000_0000) >> 29) as usize
        };
        let Some(Some(patch)) = f.assets.sprites.get(sf.lump[rot] as usize) else { return };
        let light = unsafe { m.subsector.cast::<Subsector>().as_ref() }
            .and_then(|ss| unsafe { ss.sector.as_ref() })
            .map_or(160, |s| s.lightlevel);
        let cmap = if m.frame & FF_FULLBRIGHT != 0 && f.player.fixedcolormap == 0 {
            Some(0)
        } else {
            Some(light_colormap(light, f.player.extralight, f.player.fixedcolormap))
        };
        let altitude = (m.z - m.floorz).max(0) as f32 / FRAC;
        items.push(Item::Sprite(VisSprite {
            patch,
            flip: sf.flip[rot] != 0,
            sx,
            feet: sy - altitude * cam.scale,
            ground: sy,
            cmap,
            shadow: m.flags & MF_SHADOW != 0,
        }));
    };
    unsafe { for_each_mobj(f.player.mo, &mut collect) };
}

/// Portes, portes à clé, interrupteurs et sorties, en couleur et en épais.
fn draw_usable_lines(canvas: &mut Canvas, f: &Frame, cam: &Camera) {
    for line in f.level.lines {
        let Some(kind) = usable(line.special) else { continue };
        let c = match kind {
            Usable::Door => color::DOOR,
            Usable::BlueDoor => color::BLUE_KEY,
            Usable::YellowDoor => color::YELLOW_KEY,
            Usable::RedDoor => color::RED_KEY,
            Usable::Switch => color::SWITCH,
            Usable::Exit => color::EXIT,
        };
        let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
        let (a, b) = (cam.project(v1.x, v1.y), cam.project(v2.x, v2.y));
        canvas.line(a, b, c);
        canvas.line((a.0, a.1 - 1.0), (b.0, b.1 - 1.0), c);
    }
}

fn draw_sprite(canvas: &mut Canvas, assets: &Assets, v: &VisSprite, scale: f32) {
    let p = v.patch;
    let left = if v.flip { p.width - p.left } else { p.left };
    let x0 = v.sx - left as f32 * scale;
    let y0 = v.feet - p.top as f32 * scale;
    let (w, h) = ((p.width as f32 * scale).ceil() as i32, (p.height as f32 * scale).ceil() as i32);
    let cmap = assets.colormap(if v.shadow { 26 } else { v.cmap.unwrap_or(0) });
    let (ix0, iy0) = (x0.round() as i32, y0.round() as i32);
    for dy in 0..h {
        let ty = ((dy as f32 + 0.5) / scale) as i32;
        if ty >= p.height {
            break;
        }
        for dx in 0..w {
            let mut tx = ((dx as f32 + 0.5) / scale) as i32;
            if tx >= p.width {
                break;
            }
            if v.flip {
                tx = p.width - 1 - tx;
            }
            // Spectre : une trame d'un pixel sur deux, assombrie.
            if v.shadow && (dx + dy) & 1 == 0 {
                continue;
            }
            if let Some(c) = p.pixels[(ty * p.width + tx) as usize] {
                canvas.put(ix0 + dx, iy0 + dy, cmap[c as usize]);
            }
        }
    }
}

/// Viseur en croix, `offset` relatif au centre de la vue (= le joueur).
pub fn crosshair(canvas: &mut Canvas, offset: (f32, f32)) {
    let x = (canvas.w / 2) as f32 + offset.0;
    let y = (canvas.h / 2) as f32 + offset.1;
    for (dx, dy) in [(2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)] {
        canvas.line((x + dx, y + dy), (x + dx * 2.0, y + dy * 2.0), color::CROSSHAIR);
    }
}
