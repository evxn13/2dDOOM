//! Brouillard de guerre : ce que le joueur voit depuis sa position.
//!
//! On découpe le tour du joueur en `RAYS` directions et on note, pour chacune,
//! la distance du premier mur qui bloque la vue (murs pleins, portes fermées).
//! Un point est visible s'il est plus proche que ce mur dans sa direction.

use crate::game::{Line, Sector};

const RAYS: usize = 1024;
/// Tolérance (unités) pour que le mur lui-même compte comme visible.
const MARGIN: f32 = 6.0;

pub struct Visibility {
    px: f32,
    py: f32,
    /// (Distance du premier obstacle + marge)², par direction.
    depth2: Vec<f32>,
}

/// « Angle en losange » dans [0, 4) : croît comme l'angle réel, sans trigonométrie.
#[inline]
fn diamond(dx: f32, dy: f32) -> f32 {
    if dy >= 0.0 {
        if dx >= 0.0 { dy / (dx + dy + f32::MIN_POSITIVE) } else { 1.0 - dx / (dy - dx) }
    } else if dx < 0.0 {
        2.0 - dy / (-dx - dy)
    } else {
        3.0 + dx / (dx - dy)
    }
}

#[inline]
fn bucket(dx: f32, dy: f32) -> usize {
    ((diamond(dx, dy) * (RAYS / 4) as f32) as usize).min(RAYS - 1)
}

/// Direction (non normalisée) au centre d'un secteur angulaire.
fn ray_dir(b: usize) -> (f32, f32) {
    let p = (b as f32 + 0.5) / (RAYS / 4) as f32;
    match p {
        p if p < 1.0 => (1.0 - p, p),
        p if p < 2.0 => (1.0 - p, 2.0 - p),
        p if p < 3.0 => (p - 3.0, 2.0 - p),
        p => (p - 3.0, p - 4.0),
    }
}

/// Ligne qui arrête la vue et le rayon « utiliser » : mur plein ou ouverture nulle.
pub fn blocks_sight(line: &Line) -> bool {
    if line.backsector.is_null() {
        return true;
    }
    let (f, b): (&Sector, &Sector) = unsafe { (&*line.frontsector, &*line.backsector) };
    // Ouverture nulle (porte fermée, ascenseur en haut) : rien ne passe.
    f.ceilingheight.min(b.ceilingheight) <= f.floorheight.max(b.floorheight)
}

impl Visibility {
    /// `radius` : on ignore les murs plus loin que ça (hors de l'écran).
    pub fn compute(lines: &[Line], px: f32, py: f32, radius: f32) -> Visibility {
        let mut depth2 = vec![f32::INFINITY; RAYS];
        let frac = 65536.0;
        for line in lines {
            if !blocks_sight(line) {
                continue;
            }
            let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
            let (ax, ay) = (v1.x as f32 / frac - px, v1.y as f32 / frac - py);
            let (bx, by) = (v2.x as f32 / frac - px, v2.y as f32 / frac - py);
            if ax.abs().min(bx.abs()) > radius && (ax.signum() == bx.signum())
                || ay.abs().min(by.abs()) > radius && (ay.signum() == by.signum())
            {
                continue;
            }
            let (ex, ey) = (bx - ax, by - ay);
            let (mut b0, mut b1) = (bucket(ax, ay), bucket(bx, by));
            // On parcourt l'arc le plus court entre les deux extrémités.
            if (b1 + RAYS - b0) % RAYS > RAYS / 2 {
                core::mem::swap(&mut b0, &mut b1);
            }
            let span = (b1 + RAYS - b0) % RAYS;
            for k in 0..=span {
                let b = (b0 + k) % RAYS;
                let (dx, dy) = ray_dir(b);
                let denom = dx * ey - dy * ex;
                if denom.abs() < 1e-9 {
                    continue;
                }
                // Intersection rayon (0 + t·d) / segment (a + u·e).
                let t = (ax * ey - ay * ex) / denom;
                let u = (ax * dy - ay * dx) / denom;
                if t > 0.0 && (-0.01..=1.01).contains(&u) {
                    let d2 = t * t * (dx * dx + dy * dy);
                    if d2 < depth2[b] {
                        depth2[b] = d2;
                    }
                }
            }
        }
        // Seuil final par direction, marge comprise : un seul test par point.
        for d in depth2.iter_mut() {
            *d = (d.sqrt() + MARGIN).powi(2);
        }
        Visibility { px, py, depth2 }
    }

    /// Le point (en unités de carte) est-il visible ?
    #[inline]
    pub fn sees(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.px, y - self.py);
        dx * dx + dy * dy <= self.depth2[bucket(dx, dy)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_en_losange_monotone() {
        let mut prev = -1.0;
        for i in 0..360 {
            let a = (i as f32).to_radians();
            let d = diamond(a.cos(), a.sin());
            assert!(d > prev, "{i}° : {d} <= {prev}");
            prev = d;
        }
    }

    #[test]
    fn direction_des_rayons_coherente() {
        for b in 0..RAYS {
            let (dx, dy) = ray_dir(b);
            assert_eq!(bucket(dx, dy), b);
        }
    }
}
