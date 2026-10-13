//! Convertit une capture brute 320x200 (indices de palette) en PNG,
//! avec la palette PLAYPAL lue dans le WAD.
//!
//! Usage : frame2png <capture.raw> <DOOM.WAD> <sortie.png> [facteur]
//! Le facteur optionnel agrandit l'image (pixels dupliqués), en corrigeant le
//! rapport 4:3 d'origine (pixels 1,2 fois plus hauts que larges).

use std::fs;

use doomwad::Wad;

const W: usize = 320;
const H: usize = 200;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) {
        eprintln!("usage : frame2png <capture.raw> <DOOM.WAD> <sortie.png> [facteur]");
        std::process::exit(2);
    }
    let scale: usize = args.get(4).map_or(1, |s| s.parse().expect("facteur entier attendu"));
    let raw = fs::read(&args[1]).expect("lecture de la capture");
    assert_eq!(raw.len(), W * H, "capture de taille inattendue");
    let wad = Wad::open(&args[2]).expect("lecture du WAD");
    let pal = wad.lump_by_name("PLAYPAL").expect("lump PLAYPAL introuvable");

    // Sans agrandissement, on garde l'image brute ; sinon 320x200 -> 4:3.
    let (w, h) = if scale == 1 { (W, H) } else { (W * scale, H * scale * 6 / 5) };
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let i = raw[(y * H / h) * W + x * W / w] as usize;
            rgb.extend_from_slice(&pal[i * 3..i * 3 + 3]);
        }
    }
    fs::write(&args[3], encode_png(w as u32, h as u32, &rgb)).expect("écriture du PNG");
}

/// PNG RGB 8 bits, sans filtre, compressé avec zlib.
fn encode_png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + h as usize);
    for row in rgb.chunks(w as usize * 3) {
        raw.push(0); // filtre « None »
        raw.extend_from_slice(row);
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, RGB

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", &ihdr[..]), (b"IDAT", &z[..]), (b"IEND", &[][..])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &x in data {
        crc ^= x as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
