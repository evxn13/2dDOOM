//! Contrôles « twin-stick » : clavier = déplacement par rapport à l'écran,
//! souris = viseur, clic gauche = tir.
//!
//! On lit les entrées directement dans SDL (déjà chargé par le jeu) : rien de
//! spécifique à Doom ici, la même approche vaut pour tout jeu SDL.

use core::ffi::{c_int, c_void};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::game::{BT_ATTACK, BT_USE, Ticcmd};

// Scancodes SDL : positions physiques des touches. WASD sur un clavier QWERTY
// correspond donc à ZQSD sur un clavier AZERTY.
const SC_A: usize = 4;
const SC_D: usize = 7;
const SC_E: usize = 8;
const SC_S: usize = 22;
const SC_W: usize = 26;
const SC_RIGHT: usize = 79;
const SC_LEFT: usize = 80;
const SC_DOWN: usize = 81;
const SC_UP: usize = 82;
const SC_LSHIFT: usize = 225;
const SC_RSHIFT: usize = 229;

const SDL_MOUSEMOTION: u32 = 0x400;
const SDL_BUTTON_LMASK: u32 = 1;
const SDL_BUTTON_RMASK: u32 = 4;

/// Vitesses vanilla (forwardmove) : 25 en marchant, 50 en courant.
const WALK: f32 = 25.0;
const RUN: f32 = 50.0;

type GetKeyboardState = unsafe extern "C" fn(*mut c_int) -> *const u8;
type GetMouseState = unsafe extern "C" fn(*mut c_int, *mut c_int) -> u32;
type EventFilter = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type AddEventWatch = unsafe extern "C" fn(EventFilter, *mut c_void);

struct Sdl {
    keyboard_state: GetKeyboardState,
    mouse_state: GetMouseState,
}

static SDL: OnceLock<Option<Sdl>> = OnceLock::new();

/// Position du viseur par rapport au joueur (centre de la vue), en 1/256 de pixel.
static CURSOR_X: AtomicI32 = AtomicI32::new(64 * 256);
static CURSOR_Y: AtomicI32 = AtomicI32::new(0);
/// Demi-taille de la vue, mise à jour par le rendu, pour borner le viseur.
static HALF_W: AtomicI32 = AtomicI32::new(160);
static HALF_H: AtomicI32 = AtomicI32::new(84);
/// Sensibilité souris en 1/256 de pixel de vue par pixel de souris.
static SENS: AtomicI32 = AtomicI32::new(128);

unsafe fn sym<T>(name: &core::ffi::CStr) -> Option<T> {
    let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    (!p.is_null()).then(|| unsafe { core::mem::transmute_copy(&p) })
}

/// Résolu au premier tic : SDL est alors initialisé par le jeu.
fn sdl() -> Option<&'static Sdl> {
    SDL.get_or_init(|| unsafe {
        let sdl = Sdl {
            keyboard_state: sym(c"SDL_GetKeyboardState")?,
            mouse_state: sym(c"SDL_GetMouseState")?,
        };
        let add_watch: AddEventWatch = sym(c"SDL_AddEventWatch")?;
        add_watch(on_event, core::ptr::null_mut());
        Some(sdl)
    })
    .as_ref()
}

/// Observe chaque événement SDL sans le consommer : le jeu le reçoit aussi.
unsafe extern "C" fn on_event(_user: *mut c_void, event: *mut c_void) -> c_int {
    // SDL_MouseMotionEvent : type@0 … xrel@28, yrel@32.
    let ev = event as *const u8;
    let kind = unsafe { *(ev as *const u32) };
    if kind == SDL_MOUSEMOTION {
        let xrel = unsafe { *(ev.add(28) as *const i32) };
        let yrel = unsafe { *(ev.add(32) as *const i32) };
        let s = SENS.load(Ordering::Relaxed);
        move_cursor(xrel * s, yrel * s);
    }
    1
}

fn move_cursor(dx: i32, dy: i32) {
    let hw = HALF_W.load(Ordering::Relaxed) * 256;
    let hh = HALF_H.load(Ordering::Relaxed) * 256;
    let x = (CURSOR_X.load(Ordering::Relaxed) + dx).clamp(-hw, hw);
    let y = (CURSOR_Y.load(Ordering::Relaxed) + dy).clamp(-hh, hh);
    CURSOR_X.store(x, Ordering::Relaxed);
    CURSOR_Y.store(y, Ordering::Relaxed);
}

pub fn set_sensitivity(s: f32) {
    SENS.store((s * 256.0) as i32, Ordering::Relaxed);
}

pub fn set_view_size(w: i32, h: i32) {
    HALF_W.store(w / 2, Ordering::Relaxed);
    HALF_H.store(h / 2, Ordering::Relaxed);
    move_cursor(0, 0); // re-borner si la vue a rétréci
}

/// Position du viseur en pixels de vue, relative au joueur.
pub fn cursor() -> (f32, f32) {
    (
        CURSOR_X.load(Ordering::Relaxed) as f32 / 256.0,
        CURSOR_Y.load(Ordering::Relaxed) as f32 / 256.0,
    )
}

/// État des commandes à un instant donné.
#[derive(Clone, Copy, Default)]
pub struct Controls {
    /// Direction voulue en coordonnées écran : x vers la droite, y vers le haut.
    pub move_x: f32,
    pub move_y: f32,
    pub walk: bool,
    pub fire: bool,
    /// E ou clic droit (Espace est déjà géré par le jeu).
    pub use_: bool,
}

pub fn read_controls() -> Option<Controls> {
    let sdl = sdl()?;
    let mut n: c_int = 0;
    let keys = unsafe { (sdl.keyboard_state)(&mut n) };
    if keys.is_null() {
        return None;
    }
    let keys = unsafe { core::slice::from_raw_parts(keys, n as usize) };
    let down = |sc: usize| keys.get(sc).is_some_and(|&k| k != 0);
    let axis = |neg: bool, pos: bool| (pos as i32 - neg as i32) as f32;
    let buttons = unsafe { (sdl.mouse_state)(core::ptr::null_mut(), core::ptr::null_mut()) };
    Some(Controls {
        move_x: axis(down(SC_A) || down(SC_LEFT), down(SC_D) || down(SC_RIGHT)),
        move_y: axis(down(SC_S) || down(SC_DOWN), down(SC_W) || down(SC_UP)),
        walk: down(SC_LSHIFT) || down(SC_RSHIFT),
        fire: buttons & SDL_BUTTON_LMASK != 0,
        use_: down(SC_E) || buttons & SDL_BUTTON_RMASK != 0,
    })
}

/// Angle (BAM) du joueur vers le viseur. `aspect` corrige les pixels non carrés.
pub fn aim_angle(cursor: (f32, f32), aspect: f32) -> u32 {
    let (dx, dy) = (cursor.0, -cursor.1 * aspect);
    let rad = dy.atan2(dx);
    (rad / core::f32::consts::TAU * 4294967296.0) as i64 as u32
}

/// Remplit la commande : tourner vers `target`, puis se déplacer dans la
/// direction écran demandée, quelle que soit l'orientation du joueur.
pub fn apply(cmd: &mut Ticcmd, controls: Controls, current: u32, target: u32) {
    // Le jeu applique d'abord la rotation, puis la poussée selon le nouvel angle.
    cmd.angleturn = (target.wrapping_sub(current) as i32 >> 16) as i16;
    let facing = target as f32 / 4294967296.0 * core::f32::consts::TAU;

    let (mut mx, mut my) = (controls.move_x, controls.move_y);
    let len = (mx * mx + my * my).sqrt();
    if len > 0.0 {
        mx /= len;
        my /= len;
    }
    let speed = if controls.walk { WALK } else { RUN };
    // Repère du joueur : avant = (cos, sin), droite = (sin, -cos).
    let forward = mx * facing.cos() + my * facing.sin();
    let side = mx * facing.sin() - my * facing.cos();
    cmd.forwardmove = (forward * speed).round() as i8;
    cmd.sidemove = (side * speed).round() as i8;
    if controls.fire {
        cmd.buttons |= BT_ATTACK;
    }
    if controls.use_ {
        cmd.buttons |= BT_USE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORTH: u32 = 1 << 30;
    const EAST: u32 = 0;

    fn cmd() -> Ticcmd {
        unsafe { core::mem::zeroed() }
    }

    #[test]
    fn viseur_a_droite_vise_est_et_en_haut_vise_nord() {
        assert_eq!(aim_angle((10.0, 0.0), 1.2), EAST);
        assert_eq!(aim_angle((0.0, -10.0), 1.2), NORTH);
    }

    #[test]
    fn haut_avance_vers_le_nord_meme_en_regardant_a_l_est() {
        let mut c = cmd();
        let ctl = Controls { move_y: 1.0, ..Default::default() };
        apply(&mut c, ctl, EAST, EAST);
        assert_eq!((c.forwardmove, c.sidemove), (0, -50)); // pas chassé à gauche = nord
        apply(&mut c, ctl, EAST, NORTH);
        assert_eq!((c.forwardmove, c.sidemove), (50, 0));
        assert_eq!(c.angleturn, (NORTH >> 16) as i16);
    }

    #[test]
    fn diagonale_normalisee_et_tir() {
        let mut c = cmd();
        let ctl = Controls { move_x: 1.0, move_y: 1.0, fire: true, ..Default::default() };
        apply(&mut c, ctl, EAST, EAST);
        assert_eq!((c.forwardmove, c.sidemove), (35, -35));
        assert_eq!(c.buttons & BT_ATTACK, BT_ATTACK);
    }
}
