//! Injecte des entrées clavier/souris dans un serveur X (via XTest), pour tester
//! le mod sans intervention humaine, typiquement sous Xvfb.
//!
//! Usage : xinject "<commande>; <commande>; …"
//!   sleep <ms>        attendre
//!   down <touche>     appuyer (nom de keysym X : w, a, Up, space…)
//!   up <touche>       relâcher
//!   move <dx> <dy>    mouvement relatif de la souris
//!   goto <x> <y>      position absolue de la souris
//!   press <bouton>    appuyer sur un bouton (1 = gauche)
//!   release <bouton>  relâcher un bouton

use std::ffi::{CString, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::{thread, time::Duration};

unsafe extern "C" {
    fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

struct X {
    display: *mut c_void,
    flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    string_to_keysym: unsafe extern "C" fn(*const c_char) -> c_ulong,
    keysym_to_keycode: unsafe extern "C" fn(*mut c_void, c_ulong) -> u8,
    key: unsafe extern "C" fn(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
    button: unsafe extern "C" fn(*mut c_void, c_uint, c_int, c_ulong) -> c_int,
    motion: unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_ulong) -> c_int,
    rel_motion: unsafe extern "C" fn(*mut c_void, c_int, c_int, c_ulong) -> c_int,
}

fn load(lib: &str, names: &[&str]) -> Vec<*mut c_void> {
    let l = CString::new(lib).unwrap();
    let h = unsafe { dlopen(l.as_ptr(), 2 /* RTLD_NOW */) };
    assert!(!h.is_null(), "impossible de charger {lib}");
    names
        .iter()
        .map(|n| {
            let c = CString::new(*n).unwrap();
            let p = unsafe { dlsym(h, c.as_ptr()) };
            assert!(!p.is_null(), "symbole {n} absent");
            p
        })
        .collect()
}

impl X {
    fn open() -> X {
        let x11 = load("libX11.so.6", &["XOpenDisplay", "XFlush", "XStringToKeysym", "XKeysymToKeycode"]);
        let xt = load(
            "libXtst.so.6",
            &["XTestFakeKeyEvent", "XTestFakeButtonEvent", "XTestFakeMotionEvent", "XTestFakeRelativeMotionEvent"],
        );
        unsafe {
            let open: unsafe extern "C" fn(*const c_char) -> *mut c_void = std::mem::transmute(x11[0]);
            let display = open(std::ptr::null());
            assert!(!display.is_null(), "pas de serveur X ($DISPLAY)");
            X {
                display,
                flush: std::mem::transmute(x11[1]),
                string_to_keysym: std::mem::transmute(x11[2]),
                keysym_to_keycode: std::mem::transmute(x11[3]),
                key: std::mem::transmute(xt[0]),
                button: std::mem::transmute(xt[1]),
                motion: std::mem::transmute(xt[2]),
                rel_motion: std::mem::transmute(xt[3]),
            }
        }
    }

    fn keycode(&self, name: &str) -> c_uint {
        let c = CString::new(name).unwrap();
        let sym = unsafe { (self.string_to_keysym)(c.as_ptr()) };
        assert!(sym != 0, "touche inconnue : {name}");
        unsafe { (self.keysym_to_keycode)(self.display, sym) as c_uint }
    }
}

fn main() {
    let script = std::env::args().nth(1).expect("usage : xinject \"down w; sleep 500; up w\"");
    let x = X::open();
    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = step.split_whitespace().collect();
        let num = |i: usize| -> c_long { parts[i].parse().expect("nombre attendu") };
        unsafe {
            match parts[0] {
                "sleep" => thread::sleep(Duration::from_millis(num(1) as u64)),
                "down" => _ = (x.key)(x.display, x.keycode(parts[1]), 1, 0),
                "up" => _ = (x.key)(x.display, x.keycode(parts[1]), 0, 0),
                "move" => _ = (x.rel_motion)(x.display, num(1) as c_int, num(2) as c_int, 0),
                "goto" => _ = (x.motion)(x.display, -1, num(1) as c_int, num(2) as c_int, 0),
                "press" => _ = (x.button)(x.display, num(1) as c_uint, 1, 0),
                "release" => _ = (x.button)(x.display, num(1) as c_uint, 0, 0),
                other => panic!("commande inconnue : {other}"),
            }
            (x.flush)(x.display);
        }
    }
}
