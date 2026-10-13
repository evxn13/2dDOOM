//! « Utiliser » automatique : viser la porte ou l'interrupteur à portée.

use crate::game::{Line, Mobj, usable};
use crate::visibility::blocks_sight;

/// Portée de « utiliser » dans le jeu (USERANGE = 64), un peu réduite par sécurité.
const USE_RANGE: f32 = 60.0;

fn units(v: i32) -> f32 {
    v as f32 / 65536.0
}

/// Angle (BAM) vers la ligne activable la plus proche, si elle est à portée.
///
/// Le jeu trace un rayon de 64 unités dans la direction du joueur et s'arrête au
/// premier mur : on vise donc un point de la ligne qu'aucun mur ne masque. Le
/// joueur doit aussi être du côté avant de la ligne (règle de P_UseSpecialLine).
pub unsafe fn use_target(lines: &[Line], mo: &Mobj) -> Option<u32> {
    let (px, py) = (units(mo.x), units(mo.y));
    let mut best: Option<(f32, (f32, f32))> = None;
    for (i, line) in lines.iter().enumerate() {
        if usable(line.special).is_none() {
            continue;
        }
        let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
        let (ax, ay) = (units(v1.x), units(v1.y));
        let (ex, ey) = (units(v2.x) - ax, units(v2.y) - ay);
        // Côté avant = à droite de v1 -> v2.
        if ex * (py - ay) - ey * (px - ax) > 0.0 {
            continue;
        }
        for k in 1..10 {
            let t = k as f32 / 10.0;
            let (qx, qy) = (ax + t * ex, ay + t * ey);
            let d = ((qx - px).powi(2) + (qy - py).powi(2)).sqrt();
            if d >= USE_RANGE || best.is_some_and(|(bd, _)| d >= bd) {
                continue;
            }
            let blocked = lines
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && blocks_sight(other) && unsafe { crosses(other, (px, py), (qx, qy)) });
            if !blocked {
                best = Some((d, (qx - px, qy - py)));
            }
        }
    }
    let (_, (dx, dy)) = best?;
    Some((dy.atan2(dx) / core::f32::consts::TAU * 4294967296.0) as i64 as u32)
}

/// Le segment p -> q coupe-t-il la ligne ? (q posé sur la ligne ne compte pas)
unsafe fn crosses(line: &Line, p: (f32, f32), q: (f32, f32)) -> bool {
    let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
    let (a, b) = ((units(v1.x), units(v1.y)), (units(v2.x), units(v2.y)));
    let cross = |o: (f32, f32), u: (f32, f32), w: (f32, f32)| (u.0 - o.0) * (w.1 - o.1) - (u.1 - o.1) * (w.0 - o.0);
    let (d1, d2) = (cross(a, b, p), cross(a, b, q));
    let (d3, d4) = (cross(p, q, a), cross(p, q, b));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) && d2.abs() > 1e-3
}
