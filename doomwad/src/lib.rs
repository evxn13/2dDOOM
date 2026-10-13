//! Lecture des fichiers WAD de Doom : répertoire de lumps et images « patch ».

use std::path::Path;

pub struct Lump {
    pub name: [u8; 8],
    pub pos: usize,
    pub size: usize,
}

impl Lump {
    pub fn name(&self) -> &[u8] {
        let end = self.name.iter().position(|&c| c == 0).unwrap_or(8);
        &self.name[..end]
    }
}

pub struct Wad {
    data: Vec<u8>,
    pub lumps: Vec<Lump>,
}

fn le16(b: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl Wad {
    pub fn open(path: impl AsRef<Path>) -> Result<Wad, String> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
        Wad::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Wad, String> {
        if !matches!(data.get(..4), Some(b"IWAD" | b"PWAD")) {
            return Err("pas un fichier WAD".into());
        }
        let bad = || "répertoire WAD invalide".to_string();
        let count = le32(&data, 4).ok_or_else(bad)? as usize;
        let dir = le32(&data, 8).ok_or_else(bad)? as usize;
        let mut lumps = Vec::with_capacity(count);
        for i in 0..count {
            let e = dir + i * 16;
            let entry = data.get(e..e + 16).ok_or_else(bad)?;
            let pos = le32(entry, 0).ok_or_else(bad)? as usize;
            let size = le32(entry, 4).ok_or_else(bad)? as usize;
            if pos.checked_add(size).is_none_or(|end| end > data.len()) {
                return Err(format!("lump {i} hors du fichier"));
            }
            let mut name = [0u8; 8];
            name.copy_from_slice(&entry[8..16]);
            lumps.push(Lump { name, pos, size });
        }
        Ok(Wad { data, lumps })
    }

    pub fn lump(&self, i: usize) -> Option<&[u8]> {
        let l = self.lumps.get(i)?;
        Some(&self.data[l.pos..l.pos + l.size])
    }

    /// Comme `W_CheckNumForName` : la dernière occurrence l'emporte.
    pub fn find(&self, name: &str) -> Option<usize> {
        let name = name.as_bytes();
        self.lumps.iter().rposition(|l| l.name().eq_ignore_ascii_case(name))
    }

    pub fn lump_by_name(&self, name: &str) -> Option<&[u8]> {
        self.lump(self.find(name)?)
    }
}

/// Image au format patch (sprites, textures murales), décodée en pixels.
pub struct Patch {
    pub width: i32,
    pub height: i32,
    /// Décalage du point d'ancrage depuis le bord gauche.
    pub left: i32,
    /// Hauteur de l'image au-dessus du point d'ancrage.
    pub top: i32,
    /// Indices de palette, ligne par ligne ; `None` = transparent.
    pub pixels: Vec<Option<u8>>,
}

impl Patch {
    pub fn parse(b: &[u8]) -> Option<Patch> {
        let width = le16(b, 0)? as i32;
        let height = le16(b, 2)? as i32;
        let left = le16(b, 4)? as i32;
        let top = le16(b, 6)? as i32;
        if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
            return None;
        }
        let mut pixels = vec![None; (width * height) as usize];
        for x in 0..width {
            let mut p = le32(b, 8 + x as usize * 4)? as usize;
            loop {
                let row = *b.get(p)?;
                if row == 0xff {
                    break;
                }
                let len = *b.get(p + 1)? as usize;
                let src = b.get(p + 3..p + 3 + len)?;
                for (i, &c) in src.iter().enumerate() {
                    let y = row as i32 + i as i32;
                    if y < height {
                        pixels[(y * width + x) as usize] = Some(c);
                    }
                }
                p += len + 4;
            }
        }
        Some(Patch { width, height, left, top, pixels })
    }
}

/// Texture murale composée (TEXTURE1/TEXTURE2 + PNAMES).
pub struct Texture {
    pub name: [u8; 8],
    pub width: i32,
    pub height: i32,
    /// Indices de palette, ligne par ligne ; `None` = transparent.
    pub pixels: Vec<Option<u8>>,
}

impl Texture {
    #[inline]
    pub fn pixel(&self, x: i32, y: i32) -> Option<u8> {
        let x = x.rem_euclid(self.width);
        let y = y.rem_euclid(self.height);
        self.pixels[(y * self.width + x) as usize]
    }
}

impl Wad {
    /// Toutes les textures, numérotées comme dans le jeu (TEXTURE1 puis TEXTURE2).
    /// La texture 0 signifie « pas de texture ».
    pub fn textures(&self) -> Result<Vec<Texture>, String> {
        let pnames = self.lump_by_name("PNAMES").ok_or("lump PNAMES absent")?;
        let count = le32(pnames, 0).ok_or("PNAMES invalide")? as usize;
        let mut patches = Vec::with_capacity(count);
        for i in 0..count {
            let raw = pnames.get(4 + i * 8..12 + i * 8).ok_or("PNAMES tronqué")?;
            let end = raw.iter().position(|&c| c == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&raw[..end]).into_owned();
            patches.push(self.lump_by_name(&name).and_then(Patch::parse));
        }

        let mut textures = Vec::new();
        for lump in ["TEXTURE1", "TEXTURE2"] {
            let Some(data) = self.lump_by_name(lump) else { continue };
            let n = le32(data, 0).ok_or("TEXTURE invalide")? as usize;
            for i in 0..n {
                let off = le32(data, 4 + i * 4).ok_or("TEXTURE tronqué")? as usize;
                let t = data.get(off..off + 22).ok_or("TEXTURE tronqué")?;
                let mut name = [0u8; 8];
                name.copy_from_slice(&t[..8]);
                let width = le16(t, 12).ok_or("TEXTURE")?.max(1) as i32;
                let height = le16(t, 14).ok_or("TEXTURE")?.max(1) as i32;
                let patch_count = le16(t, 20).ok_or("TEXTURE")?.max(0) as usize;
                let mut pixels = vec![None; (width * height) as usize];
                for p in 0..patch_count {
                    let at = off + 22 + p * 10;
                    let (Some(ox), Some(oy), Some(idx)) = (le16(data, at), le16(data, at + 2), le16(data, at + 4)) else {
                        break;
                    };
                    let Some(Some(patch)) = patches.get(idx as usize) else { continue };
                    for py in 0..patch.height {
                        for px in 0..patch.width {
                            let (x, y) = (ox as i32 + px, oy as i32 + py);
                            if x < 0 || y < 0 || x >= width || y >= height {
                                continue;
                            }
                            if let Some(c) = patch.pixels[(py * patch.width + px) as usize] {
                                pixels[(y * width + x) as usize] = Some(c);
                            }
                        }
                    }
                }
                textures.push(Texture { name, width, height, pixels });
            }
        }
        Ok(textures)
    }
}
