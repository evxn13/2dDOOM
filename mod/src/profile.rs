//! Adresses des fonctions et variables de Chocolate Doom utilisées par le mod.
//!
//! Elles sont d'abord cherchées par nom dans la table des symboles de
//! l'exécutable, ce qui marche pour n'importe quelle compilation non strippée.
//! À défaut, on se rabat sur les offsets trouvés par rétro-ingénierie (REA +
//! Ghidra) sur un binaire strippé précis, reconnu à sa signature.

use std::collections::HashMap;

use crate::hook::MainModule;

macro_rules! addresses {
    ($( $(#[$doc:meta])* $field:ident = $symbol:literal @ $offset:literal; )*) => {
        /// Adresses absolues dans le processus.
        pub struct Addresses {
            $( $(#[$doc])* pub $field: usize, )*
        }

        impl Addresses {
            fn from_symbols(base: usize, symbols: &HashMap<String, u64>) -> Option<Addresses> {
                Some(Addresses { $( $field: base + *symbols.get($symbol)? as usize, )* })
            }

            fn from_known_build(base: usize) -> Addresses {
                Addresses { $( $field: base + $offset, )* }
            }
        }
    };
}

// Chemins de découverte dans le binaire strippé (adresses Ghidra = offset + 0x100000).
addresses! {
    /// `void R_RenderPlayerView(player_t*)` : chaîne "R_DrawPlanes: drawsegs
    /// overflow" -> R_DrawPlanes -> son unique appelant.
    render_player_view = "R_RenderPlayerView" @ 0x59170;
    /// `void G_BuildTiccmd(ticcmd_t*, int)`.
    build_ticcmd = "G_BuildTiccmd" @ 0x3c5d0;
    /// `loop_interface_t doom_loop_interface` = { ProcessEvents, BuildTiccmd,
    /// RunTic, RunMenu } : chaîne "NET GAME" -> G_Ticker -> RunTic -> relocation.
    loop_interface = "doom_loop_interface" @ 0xc3b00;
    /// `byte* I_VideoBuffer` (320x200, 8 bits) et la zone de vue : R_InitBuffer.
    video_buffer = "I_VideoBuffer" @ 0xdc430;
    viewwindowx = "viewwindowx" @ 0x15a5a4;
    viewwindowy = "viewwindowy" @ 0x15a5a0;
    viewheight = "viewheight" @ 0x15a5a8;
    scaledviewwidth = "scaledviewwidth" @ 0x15a5ac;
    /// Lignes et secteurs : P_SpawnSpecials ("Too many scrolling wall linedefs").
    lines = "lines" @ 0x156c08;
    numlines = "numlines" @ 0x156c10;
    sectors = "sectors" @ 0x156c38;
    numsectors = "numsectors" @ 0x156c40;
    /// BSP : R_PointInSubsector et R_Subsector ("R_Subsector: ss %i…").
    nodes = "nodes" @ 0x156c18;
    numnodes = "numnodes" @ 0x156c20;
    subsectors = "subsectors" @ 0x156c28;
    numsubsectors = "numsubsectors" @ 0x156c30;
    segs = "segs" @ 0x156c48;
    /// Sols : R_DrawPlanes (lump = firstflat + flattranslation[pic]).
    flattranslation = "flattranslation" @ 0x157628;
    skyflatnum = "skyflatnum" @ 0x185224;
    /// Sprites : R_ProjectSprite ("R_ProjectSprite: invalid sprite number").
    sprites = "sprites" @ 0x187e70;
    numsprites = "numsprites" @ 0x187e6c;
}

/// Début de R_RenderPlayerView dans le binaire de référence (indépendant de l'ASLR) :
/// `sub rsp, 8 ; call R_SetupFrame ; call R_ClearClipSegs`.
const KNOWN_BUILD_SIGNATURE: &[u8] = &[0x48, 0x83, 0xec, 0x08, 0xe8, 0x17, 0xff, 0xff, 0xff, 0xe8, 0x82, 0xdd];

/// Taille du saut écrit au début de R_RenderPlayerView.
pub const PATCH_LEN: usize = 12;

impl Addresses {
    /// Case de `doom_loop_interface` qui pointe vers G_BuildTiccmd.
    pub fn build_ticcmd_slot(&self) -> usize {
        self.loop_interface + size_of::<usize>()
    }

    /// `None` si l'exécutable n'est pas Chocolate Doom (ou une version inconnue).
    pub fn resolve(module: &MainModule) -> Option<(Addresses, &'static str)> {
        let base = module.base;
        if let Some(addr) = Addresses::from_symbols(base, &crate::symbols::load_own())
            && module.is_code(addr.render_player_view, PATCH_LEN)
        {
            return Some((addr, "symboles de l'exécutable"));
        }
        let known = Addresses::from_known_build(base);
        module
            .code_matches(known.render_player_view, KNOWN_BUILD_SIGNATURE)
            .then_some((known, "profil REA du binaire de référence"))
    }
}
