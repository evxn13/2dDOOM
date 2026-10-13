//! Vues Rust des structures de Doom (x86_64), reconstituées à partir du code
//! décompilé et vérifiées contre les infos de débogage (`ptype /o`).
//! Les structures sont partielles : on ne déclare que ce qu'on lit.

use core::ffi::c_void;
use core::mem::{offset_of, size_of};

/// Unité de Doom : virgule fixe 16.16.
pub const FRACBITS: u32 = 16;

#[repr(C)]
pub struct Thinker {
    pub prev: *mut Thinker,
    pub next: *mut Thinker,
    /// Pointeur de fonction ; vaut `P_MobjThinker` pour un objet du monde,
    /// -1 pour un thinker supprimé.
    pub function: usize,
}

#[repr(C)]
pub struct Mobj {
    pub thinker: Thinker,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub snext: *mut c_void,
    pub sprev: *mut c_void,
    /// Angle binaire (BAM) : 0 = est, 2^30 = nord.
    pub angle: u32,
    pub sprite: i32,
    pub frame: i32,
    pub bnext: *mut c_void,
    pub bprev: *mut c_void,
    pub subsector: *mut c_void,
    pub floorz: i32,
    pub ceilingz: i32,
    pub radius: i32,
    pub height: i32,
    pub momx: i32,
    pub momy: i32,
    pub momz: i32,
    pub validcount: i32,
    pub kind: i32,
    pub info: *mut c_void,
    pub tics: i32,
    pub state: *mut c_void,
    pub flags: u32,
    pub health: i32,
}


/// `player_t` (partiel).
#[repr(C)]
pub struct Player {
    pub mo: *mut Mobj,
    _skip: [u8; 0xf8],
    /// Lumière ajoutée par le flash des armes.
    pub extralight: i32,
    /// Colormap forcée (0 = aucune) : invulnérabilité, lunettes de vision nocturne.
    pub fixedcolormap: i32,
}

/// Bit de `frame` : sprite toujours en pleine lumière (flammes, projectiles…).
pub const FF_FULLBRIGHT: i32 = 0x8000;
pub const FF_FRAMEMASK: i32 = 0x7fff;
pub const MF_SHADOW: u32 = 0x40000;

#[repr(C)]
pub struct Vertex {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
pub struct Sector {
    pub floorheight: i32,
    pub ceilingheight: i32,
    pub floorpic: i16,
    pub ceilingpic: i16,
    pub lightlevel: i16,
    _rest: [u8; 0x80 - 14],
}

/// Face d'une ligne : textures du haut, du bas et du milieu.
#[repr(C)]
pub struct Side {
    pub textureoffset: i32,
    pub rowoffset: i32,
    pub toptexture: i16,
    pub bottomtexture: i16,
    pub midtexture: i16,
    pub sector: *const Sector,
}

/// Rôle d'une ligne activable avec « utiliser » (spéciaux vanilla Doom / Doom II).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Usable {
    Door,
    BlueDoor,
    YellowDoor,
    RedDoor,
    Switch,
    Exit,
}

pub fn usable(special: i16) -> Option<Usable> {
    use Usable::*;
    Some(match special {
        1 | 31 | 117 | 118 => Door,
        26 | 32 | 99 | 133 => BlueDoor,
        27 | 34 | 136 | 137 => YellowDoor,
        28 | 33 | 134 | 135 => RedDoor,
        11 | 51 => Exit,
        7 | 9 | 14 | 15 | 18 | 20 | 21 | 23 | 29 | 41 | 42 | 43 | 45 | 49 | 50 | 55 | 60..=71 | 101..=103
        | 111..=116 | 122 | 123 | 127 | 131 | 132 | 138..=140 => Switch,
        _ => return None,
    })
}

/// Nœud du BSP : une droite de partage et deux enfants.
#[repr(C)]
pub struct Node {
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    pub bbox: [[i32; 4]; 2],
    /// Bit 0x8000 = l'enfant est un sous-secteur.
    pub children: [u16; 2],
}

pub const NF_SUBSECTOR: u16 = 0x8000;

/// Zone convexe de la carte, feuille du BSP.
#[repr(C)]
pub struct Subsector {
    pub sector: *const Sector,
    pub numlines: i16,
    pub firstline: i16,
}

/// Morceau de ligne qui borde un sous-secteur (le secteur est à sa droite).
#[repr(C)]
pub struct Seg {
    pub v1: *const Vertex,
    pub v2: *const Vertex,
    pub offset: i32,
    pub angle: u32,
    pub sidedef: *const Side,
    pub linedef: *const Line,
    pub frontsector: *const Sector,
    pub backsector: *const Sector,
}

#[repr(C)]
pub struct SpriteDef {
    pub numframes: i32,
    pub spriteframes: *const SpriteFrame,
}

#[repr(C)]
pub struct SpriteFrame {
    /// 0 = même image sous tous les angles.
    pub rotate: i32,
    /// Numéro de lump relatif au premier sprite (S_START + 1).
    pub lump: [i16; 8],
    pub flip: [u8; 8],
}

#[repr(C)]
pub struct Line {
    pub v1: *const Vertex,
    pub v2: *const Vertex,
    pub dx: i32,
    pub dy: i32,
    pub flags: i16,
    pub special: i16,
    pub tag: i16,
    pub sidenum: [i16; 2],
    pub bbox: [i32; 4],
    pub slopetype: i32,
    pub frontsector: *const Sector,
    pub backsector: *const Sector,
    pub validcount: i32,
    pub specialdata: *mut c_void,
}

pub const ML_SECRET: i16 = 0x20;

/// Commande envoyée au jeu à chaque tic (35 par seconde).
#[repr(C)]
pub struct Ticcmd {
    /// Positif = avancer. Vanilla : 25 en marche, 50 en course.
    pub forwardmove: i8,
    /// Positif = pas chassé à droite.
    pub sidemove: i8,
    /// Ajouté à l'angle du joueur après décalage de 16 bits.
    pub angleturn: i16,
    pub chatchar: u8,
    pub buttons: u8,
    pub consistancy: u8,
    pub buttons2: u8,
    pub inventory: i32,
    pub lookfly: u8,
    pub arti: u8,
}

pub const BT_ATTACK: u8 = 1;
pub const BT_USE: u8 = 2;

// Si la mise en page diverge du binaire, on refuse de compiler.
const _: () = {
    assert!(offset_of!(Mobj, x) == 0x18);
    assert!(offset_of!(Mobj, angle) == 0x38);
    assert!(offset_of!(Mobj, radius) == 104);
    assert!(offset_of!(Mobj, flags) == 160);
    assert!(offset_of!(Mobj, health) == 164);
    assert!(offset_of!(Line, special) == 0x1a);
    assert!(offset_of!(Line, bbox) == 36);
    assert!(offset_of!(Line, frontsector) == 56);
    assert!(offset_of!(Line, backsector) == 64);
    assert!(size_of::<Line>() == 0x58);
    assert!(offset_of!(Ticcmd, buttons) == 5);
    assert!(size_of::<Ticcmd>() == 16);
    assert!(offset_of!(Player, extralight) == 0x100);
    assert!(offset_of!(Player, fixedcolormap) == 0x104);
    assert!(offset_of!(Mobj, sprite) == 0x3c);
    assert!(offset_of!(Mobj, frame) == 0x40);
    assert!(offset_of!(Sector, lightlevel) == 12);
    assert!(size_of::<Sector>() == 0x80);
    assert!(size_of::<Node>() == 0x34);
    assert!(offset_of!(Node, children) == 0x30);
    assert!(size_of::<Subsector>() == 0x10);
    assert!(size_of::<Seg>() == 0x38);
    assert!(size_of::<Side>() == 24);
    assert!(offset_of!(Side, midtexture) == 12);
    assert!(offset_of!(Seg, backsector) == 48);
    assert!(size_of::<SpriteDef>() == 0x10);
    assert!(size_of::<SpriteFrame>() == 0x1c);
    assert!(offset_of!(SpriteFrame, flip) == 0x14);
};

/// Itère sur tous les objets du monde en suivant la liste chaînée des thinkers.
///
/// Pas besoin de connaître l'adresse de `thinkercap` : on part du mobj du joueur
/// et on garde les thinkers qui ont la même fonction que lui (`P_MobjThinker`).
pub unsafe fn for_each_mobj(start: *mut Mobj, mut f: impl FnMut(&Mobj)) {
    let start = start as *mut Thinker;
    let mobj_fn = unsafe { (*start).function };
    let mut t = start;
    // Garde-fou contre une liste corrompue.
    for _ in 0..65536 {
        let th = unsafe { &*t };
        if th.function == mobj_fn {
            f(unsafe { &*(t as *const Mobj) });
        }
        t = th.next;
        if t.is_null() || t == start {
            break;
        }
    }
}
