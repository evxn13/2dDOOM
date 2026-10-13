//! Adresses découvertes par rétro-ingénierie (REA + Ghidra) pour un binaire précis.
//!
//! Tous les offsets sont relatifs à l'adresse de chargement de l'exécutable (PIE).
//! Ghidra charge le binaire à 0x100000 : une adresse Ghidra 0x159170 donne ici 0x59170.

pub struct Profile {
    pub name: &'static str,

    /// `R_RenderPlayerView(player_t*)` — trouvée via la chaîne
    /// "R_DrawPlanes: drawsegs overflow" -> R_DrawPlanes -> unique appelant.
    pub render_player_view: usize,
    /// Premiers octets attendus à `render_player_view` (indépendants de l'ASLR).
    /// Ce sont aussi les octets écrasés par notre saut.
    pub render_signature: &'static [u8],

    /// `byte* I_VideoBuffer` (320x200, 8 bits palettisé) — lu par R_InitBuffer.
    pub video_buffer: usize,
    /// `int viewwindowx / viewwindowy` — écrits par R_InitBuffer.
    pub viewwindowx: usize,
    pub viewwindowy: usize,
    /// `int viewheight / scaledviewwidth` — voisins de viewwindowx (confirmés via symboles).
    pub viewheight: usize,
    pub scaledviewwidth: usize,

    /// `line_t* lines` / `int numlines` — parcourus par P_SpawnSpecials
    /// (chaîne "Too many scrolling wall linedefs").
    pub lines: usize,
    pub numlines: usize,
    /// `sector_t* sectors` / `int numsectors` — parcourus par P_SpawnSpecials.
    pub sectors: usize,
    pub numsectors: usize,

    /// `G_BuildTiccmd(ticcmd_t *cmd, int maketic)` — construit la commande du joueur.
    /// Chemin : chaîne "NET GAME" -> G_Ticker -> unique appelant RunTic ->
    /// relocation R_X86_64_RELATIVE vers RunTic dans `doom_loop_interface`.
    pub build_ticcmd: usize,
    /// Case de `doom_loop_interface` (dans .data, inscriptible) qui pointe vers
    /// G_BuildTiccmd : { ProcessEvents, BuildTiccmd, RunTic, RunMenu }.
    pub build_ticcmd_slot: usize,

    /// BSP — tous lus par `R_PointInSubsector` (unique fonction qui lit numnodes
    /// en dehors du rendu) et `R_Subsector` (chaîne "R_Subsector: ss %i…").
    pub nodes: usize,
    pub numnodes: usize,
    pub subsectors: usize,
    pub numsubsectors: usize,
    pub segs: usize,

    /// `int* flattranslation` et `int skyflatnum` — lus par R_DrawPlanes :
    /// lump du sol = firstflat + flattranslation[floorpic] (animations comprises).
    pub flattranslation: usize,
    pub skyflatnum: usize,

    /// `spritedef_t* sprites` / `int numsprites` — lus par R_ProjectSprite
    /// (chaîne "R_ProjectSprite: invalid sprite number").
    pub sprites: usize,
    pub numsprites: usize,
}

pub const CHOCOLATE_DOOM_3_1_1: Profile = Profile {
    name: "chocolate-doom 3.1.1 (x86_64, RelWithDebInfo)",
    render_player_view: 0x59170,
    render_signature: &[
        0x48, 0x83, 0xec, 0x08, // sub rsp, 8
        0xe8, 0x17, 0xff, 0xff, 0xff, // call R_SetupFrame
        0xe8, 0x82, 0xdd, // call R_ClearClipSegs (début)
    ],
    video_buffer: 0xdc430,
    viewwindowx: 0x15a5a4,
    viewwindowy: 0x15a5a0,
    viewheight: 0x15a5a8,
    scaledviewwidth: 0x15a5ac,
    lines: 0x156c08,
    numlines: 0x156c10,
    sectors: 0x156c38,
    numsectors: 0x156c40,
    build_ticcmd: 0x3c5d0,
    build_ticcmd_slot: 0xc3b08,
    nodes: 0x156c18,
    numnodes: 0x156c20,
    subsectors: 0x156c28,
    numsubsectors: 0x156c30,
    segs: 0x156c48,
    flattranslation: 0x157628,
    skyflatnum: 0x185224,
    sprites: 0x187e70,
    numsprites: 0x187e6c,
};
