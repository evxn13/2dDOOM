//! Accès à la carte en mémoire : recherche d'un point dans le BSP.

use crate::game::*;

pub struct Level<'a> {
    pub lines: &'a [Line],
    pub nodes: &'a [Node],
    pub subsectors: &'a [Subsector],
    pub segs: *const Seg,
}

impl Level<'_> {
    /// Équivalent de `R_PointInSubsector` (coordonnées en virgule fixe).
    pub fn subsector_at(&self, x: i32, y: i32) -> Option<&Subsector> {
        if self.nodes.is_empty() {
            return self.subsectors.first();
        }
        let mut n = self.nodes.len() as u16 - 1;
        // Garde-fou : un BSP valide a une profondeur bien inférieure.
        for _ in 0..256 {
            if n & NF_SUBSECTOR != 0 {
                return self.subsectors.get((n & !NF_SUBSECTOR) as usize);
            }
            let node = self.nodes.get(n as usize)?;
            n = node.children[point_on_side(x, y, node)];
        }
        None
    }

    /// Vrai si le point est hors de la carte : derrière un mur plein de sa zone.
    /// Le BSP couvre tout le plan, y compris l'extérieur des murs.
    pub fn is_void(&self, ss: &Subsector, x: i32, y: i32) -> bool {
        for i in 0..ss.numlines.max(0) as usize {
            let seg = unsafe { &*self.segs.add(ss.firstline as usize + i) };
            if !seg.backsector.is_null() {
                continue;
            }
            let (v1, v2) = unsafe { (&*seg.v1, &*seg.v2) };
            // Le secteur est à droite du segment v1 -> v2.
            let cross = (v2.x as i64 - v1.x as i64) * (y as i64 - v1.y as i64)
                - (v2.y as i64 - v1.y as i64) * (x as i64 - v1.x as i64);
            if cross > 0 {
                return true;
            }
        }
        false
    }
}

/// 0 = devant (à droite de la droite de partage), 1 = derrière.
fn point_on_side(x: i32, y: i32, node: &Node) -> usize {
    let dx = x as i64 - node.x as i64;
    let dy = y as i64 - node.y as i64;
    (dy * node.dx as i64 >= node.dy as i64 * dx) as usize
}
