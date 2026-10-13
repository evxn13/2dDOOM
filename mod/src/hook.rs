//! Mécanique d'injection : adresse de chargement et détour par saut absolu.

use core::ffi::{c_int, c_void};

/// L'exécutable principal tel que chargé en mémoire.
pub struct MainModule {
    pub base: usize,
    /// Plages [début, fin) des segments exécutables.
    exec_ranges: Vec<(usize, usize)>,
}

impl MainModule {
    /// Premier module de `dl_iterate_phdr` = l'exécutable principal.
    pub fn find() -> Option<MainModule> {
        unsafe extern "C" fn first(info: *mut libc::dl_phdr_info, _size: usize, out: *mut c_void) -> c_int {
            let info = unsafe { &*info };
            let module = unsafe { &mut *(out as *mut MainModule) };
            module.base = info.dlpi_addr as usize;
            let phdrs = unsafe { core::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize) };
            for ph in phdrs {
                if ph.p_type == libc::PT_LOAD && ph.p_flags & libc::PF_X != 0 {
                    let start = module.base + ph.p_vaddr as usize;
                    module.exec_ranges.push((start, start + ph.p_memsz as usize));
                }
            }
            1 // on s'arrête au premier
        }
        let mut module = MainModule { base: 0, exec_ranges: Vec::new() };
        unsafe { libc::dl_iterate_phdr(Some(first), &mut module as *mut MainModule as *mut c_void) };
        // Un PIE n'est jamais chargé à 0.
        (module.base != 0).then_some(module)
    }

    /// `[addr, addr + len)` est-il entièrement dans un segment exécutable ?
    pub fn is_code(&self, addr: usize, len: usize) -> bool {
        self.exec_ranges.iter().any(|&(s, e)| addr >= s && addr + len <= e)
    }

    /// Vérifie que `expected` se trouve à `addr`, sans lire hors du code exécutable :
    /// sous `LD_PRELOAD`, on est aussi chargé dans d'autres programmes.
    pub fn code_matches(&self, addr: usize, expected: &[u8]) -> bool {
        self.is_code(addr, expected.len())
            && unsafe { core::slice::from_raw_parts(addr as *const u8, expected.len()) } == expected
    }
}

/// Écrase le début de `target` par `movabs rax, dest ; jmp rax` (12 octets).
/// La fonction d'origine n'est plus jamais exécutée : pas besoin de trampoline.
pub unsafe fn install_jump(target: usize, dest: usize) -> Result<(), String> {
    let mut patch = [0u8; crate::profile::PATCH_LEN];
    patch[0] = 0x48;
    patch[1] = 0xb8;
    patch[2..10].copy_from_slice(&(dest as u64).to_le_bytes());
    patch[10] = 0xff;
    patch[11] = 0xe0;

    let page = target & !0xfff;
    let len = (target + patch.len() - page).next_multiple_of(0x1000);
    unsafe {
        if libc::mprotect(page as *mut c_void, len, libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC) != 0 {
            return Err(format!("mprotect RWX: {}", std::io::Error::last_os_error()));
        }
        core::ptr::copy_nonoverlapping(patch.as_ptr(), target as *mut u8, patch.len());
        if libc::mprotect(page as *mut c_void, len, libc::PROT_READ | libc::PROT_EXEC) != 0 {
            return Err(format!("mprotect RX: {}", std::io::Error::last_os_error()));
        }
    }
    Ok(())
}
