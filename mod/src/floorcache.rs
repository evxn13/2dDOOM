//! Cache « quel secteur à cet endroit ? » par tuiles de la carte.
//!
//! Chercher dans le BSP pour chaque pixel coûte cher. La géométrie d'un niveau
//! ne change pas (seuls les hauteurs, textures et lumières bougent, et on les lit
//! en direct), donc on mémorise le secteur de chaque case de 2x2 unités, tuile
//! par tuile, à la demande.

use std::collections::HashMap;

use crate::game::Sector;
use crate::level::Level;

/// Taille d'une case en unités de carte (puissance de 2).
pub const CELL_SHIFT: i32 = 1;
/// Cases par côté de tuile (puissance de 2).
const TILE_SHIFT: i32 = 5;
const TILE: i32 = 1 << TILE_SHIFT;

/// Valeur de case « hors de la carte ».
pub const VOID: u16 = u16::MAX;

pub struct FloorCache {
    /// Identité du niveau : adresse et taille du BSP.
    level_key: (usize, usize),
    tiles: HashMap<(i32, i32), Box<[u16]>>,
}

impl FloorCache {
    pub fn new() -> FloorCache {
        FloorCache { level_key: (0, 0), tiles: HashMap::new() }
    }

    /// Vide le cache quand le niveau change.
    pub fn sync(&mut self, level: &Level) {
        let key = (level.nodes.as_ptr() as usize, level.nodes.len());
        if key != self.level_key {
            self.level_key = key;
            self.tiles.clear();
        }
    }

    /// Tuile contenant la case (cx, cy), calculée si besoin.
    pub fn tile(&mut self, level: &Level, sectors: &[Sector], tx: i32, ty: i32) -> &[u16] {
        self.tiles.entry((tx, ty)).or_insert_with(|| build_tile(level, sectors, tx, ty))
    }
}

/// Coordonnées de tuile et indice dans la tuile pour une case.
#[inline]
pub fn split(cell_x: i32, cell_y: i32) -> ((i32, i32), usize) {
    let tile = (cell_x >> TILE_SHIFT, cell_y >> TILE_SHIFT);
    let idx = ((cell_y & (TILE - 1)) * TILE + (cell_x & (TILE - 1))) as usize;
    (tile, idx)
}

fn build_tile(level: &Level, sectors: &[Sector], tx: i32, ty: i32) -> Box<[u16]> {
    let mut cells = vec![VOID; (TILE * TILE) as usize].into_boxed_slice();
    let base = sectors.as_ptr() as usize;
    let size = size_of::<Sector>();
    for j in 0..TILE {
        for i in 0..TILE {
            // Centre de la case, en virgule fixe.
            let cx = (tx << TILE_SHIFT) + i;
            let cy = (ty << TILE_SHIFT) + j;
            let half = 1 << (CELL_SHIFT - 1);
            let x = ((cx << CELL_SHIFT) + half) << 16;
            let y = ((cy << CELL_SHIFT) + half) << 16;
            let Some(ss) = level.subsector_at(x, y) else { continue };
            if level.is_void(ss, x, y) {
                continue;
            }
            let offset = (ss.sector as usize).wrapping_sub(base);
            if offset % size == 0 && offset / size < sectors.len() {
                cells[(j * TILE + i) as usize] = (offset / size) as u16;
            }
        }
    }
    cells
}
