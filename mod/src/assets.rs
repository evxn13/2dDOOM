//! Ressources graphiques lues directement dans le WAD utilisé par le jeu.

use doomwad::{Patch, Texture, Wad};

pub struct Assets {
    pub wad: Wad,
    /// 34 tables de 256 couleurs : 0 = pleine lumière … 31 = noir, 32 = invulnérabilité.
    pub colormaps: Vec<u8>,
    /// Premier lump après F_START : référence des numéros de sols (`firstflat`).
    pub first_flat: usize,
    /// Sprites décodés, indexés comme dans le jeu : lump - (S_START + 1).
    pub sprites: Vec<Option<Patch>>,
    /// Textures murales, numérotées comme dans le jeu (0 = aucune).
    pub textures: Vec<Texture>,
}

/// Le chemin du WAD : `TOPDOOM_WAD`, sinon l'argument `-iwad` du jeu.
fn wad_path() -> Option<String> {
    if let Ok(p) = std::env::var("TOPDOOM_WAD") {
        return Some(p);
    }
    let cmdline = std::fs::read("/proc/self/cmdline").ok()?;
    let args: Vec<&[u8]> = cmdline.split(|&b| b == 0).collect();
    let i = args.iter().position(|a| a.eq_ignore_ascii_case(b"-iwad"))?;
    Some(String::from_utf8_lossy(args.get(i + 1)?).into_owned())
}

impl Assets {
    pub fn load() -> Result<Assets, String> {
        let path = wad_path().ok_or("WAD introuvable (passer -iwad ou TOPDOOM_WAD)")?;
        let wad = Wad::open(&path)?;
        let colormaps = wad.lump_by_name("COLORMAP").ok_or("lump COLORMAP absent")?.to_vec();
        if colormaps.len() < 34 * 256 {
            return Err("COLORMAP trop court".into());
        }
        let first_flat = wad.find("F_START").ok_or("marqueur F_START absent")? + 1;
        let first_sprite = wad.find("S_START").ok_or("marqueur S_START absent")? + 1;
        let last_sprite = wad.find("S_END").ok_or("marqueur S_END absent")?;
        let sprites = (first_sprite..last_sprite).map(|i| wad.lump(i).and_then(Patch::parse)).collect();
        let textures = wad.textures()?;
        eprintln!("[topdoom] ressources chargées depuis {path} ({} textures)", textures.len());
        Ok(Assets { wad, colormaps, first_flat, sprites, textures })
    }

    pub fn colormap(&self, index: usize) -> &[u8] {
        let i = index.min(33);
        &self.colormaps[i * 256..i * 256 + 256]
    }

    /// Texture de sol 64x64, ou `None` si le lump n'est pas un flat.
    pub fn flat(&self, flatnum: i32) -> Option<&[u8]> {
        let lump = self.wad.lump(self.first_flat.checked_add_signed(flatnum as isize)?)?;
        (lump.len() >= 4096).then_some(lump)
    }
}

/// Colormap d'un secteur vu du dessus (la distance est fixe, contrairement à la 3D).
pub fn light_colormap(lightlevel: i16, extralight: i32, fixed: i32) -> usize {
    if fixed != 0 {
        return fixed as usize;
    }
    let level = ((lightlevel as i32) >> 4) + extralight; // 0..15 en vanilla
    ((15 - level.clamp(0, 15)) * 2) as usize
}

/// Colormap d'une zone hors de vue (brouillard de guerre).
pub fn fogged(cmap: usize) -> usize {
    if cmap >= 32 { cmap } else { (cmap + 12).min(29) }
}
