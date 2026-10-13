//! Convertit une capture brute 320x200 (indices de palette) en PNG,
//! avec la palette PLAYPAL lue dans le WAD.
//!
//! Usage : frame2png <capture.raw> <DOOM.WAD> <sortie.png>

use std::fs;

use doomwad::Wad;

const W: usize = 320;
const H: usize = 200;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage : frame2png <capture.raw> <DOOM.WAD> <sortie.png>");
        std::process::exit(2);
    }
    let raw = fs::read(&args[1]).expect("lecture de la capture");
    assert_eq!(raw.len(), W * H, "capture de taille inattendue");
    let wad = Wad::open(&args[2]).expect("lecture du WAD");
    let pal = wad.lump_by_name("PLAYPAL").expect("lump PLAYPAL introuvable");

    let mut rgb = Vec::with_capacity(W * H * 3);
    for &i in &raw {
        rgb.extend_from_slice(&pal[i as usize * 3..i as usize * 3 + 3]);
    }
    fs::write(&args[3], encode_png(W as u32, H as u32, &rgb)).expect("écriture du PNG");
}

/// PNG minimal sans dépendance : deflate en blocs « stockés » (non compressés).
fn encode_png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + h as usize);
    for row in rgb.chunks(w as usize * 3) {
        raw.push(0); // filtre « None »
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        z.push(chunks.peek().is_none() as u8);
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

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

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
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
