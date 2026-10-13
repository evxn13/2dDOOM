//! Lecture de la table des symboles (`.symtab`) de l'exécutable en cours.

use std::collections::HashMap;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

const SHT_SYMTAB: u32 = 2;

/// Symboles de l'exécutable (nom -> adresse virtuelle avant relocation).
/// Vide si le binaire a été strippé.
pub fn load_own() -> HashMap<String, u64> {
    std::fs::read("/proc/self/exe").ok().and_then(|elf| parse(&elf)).unwrap_or_default()
}

/// ELF64 little-endian uniquement.
fn parse(elf: &[u8]) -> Option<HashMap<String, u64>> {
    if elf.get(..5)? != b"\x7fELF\x02" {
        return None;
    }
    let shoff = u64_at(elf, 0x28)? as usize;
    let shentsize = u16_at(elf, 0x3a)? as usize;
    let shnum = u16_at(elf, 0x3c)? as usize;
    let section = |i: usize| shoff + i * shentsize;

    let symtab = (0..shnum).find(|&i| u32_at(elf, section(i) + 4) == Some(SHT_SYMTAB))?;
    let (sym_off, sym_size) = (u64_at(elf, section(symtab) + 0x18)? as usize, u64_at(elf, section(symtab) + 0x20)? as usize);
    let strtab = u32_at(elf, section(symtab) + 0x28)? as usize;
    let str_off = u64_at(elf, section(strtab) + 0x18)? as usize;

    let mut symbols = HashMap::new();
    for entry in (sym_off..sym_off + sym_size).step_by(24) {
        let name_off = u32_at(elf, entry)? as usize;
        let value = u64_at(elf, entry + 8)?;
        let name = elf.get(str_off + name_off..)?;
        let name = &name[..name.iter().position(|&c| c == 0)?];
        if value != 0 && !name.is_empty() {
            symbols.insert(String::from_utf8_lossy(name).into_owned(), value);
        }
    }
    Some(symbols)
}
