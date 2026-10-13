//! 2dDOOM : remplace le rendu 3D de Chocolate Doom par une vue du dessus.
//!
//! Chargé via `LD_PRELOAD`. Au démarrage, on retrouve les adresses du jeu,
//! puis on détourne deux fonctions :
//! - `R_RenderPlayerView` (rendu de la vue) vers notre rendu vu du dessus ;
//! - `G_BuildTiccmd` (commande du joueur) pour les contrôles clavier + souris.
//!
//! Variables d'environnement :
//! - `DOOM2D_ZOOM`        unités de carte par pixel (défaut 2)
//! - `DOOM2D_WALL_HEIGHT` hauteur max des murs en relief, en unités (défaut 32)
//! - `DOOM2D_FOG`         0 pour désactiver le brouillard de guerre
//! - `DOOM2D_MOUSE_SENS`  pixels de viseur par pixel de souris (défaut 0.5)
//! - `DOOM2D_WAD`         WAD à lire (défaut : l'argument -iwad du jeu)
//! - `DOOM2D_DEBUG`       afficher les commandes envoyées au jeu
//! - `DOOM2D_PROFILE`     afficher le temps passé dans chaque étape du rendu
//! - `DOOM2D_DUMP`        préfixe de fichier pour capturer des images brutes
//! - `DOOM2D_DUMP_FRAMES` numéros d'images à capturer, ex. "100,300"
//! - `DOOM2D_DUMP_EVERY`  capturer une image toutes les N

mod assets;
mod floorcache;
mod game;
mod hook;
mod input;
mod interact;
mod level;
mod profile;
mod render;
mod symbols;
mod visibility;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};

use assets::Assets;
use game::{Line, Node, Player, Sector, Seg, SpriteDef, Subsector, Ticcmd};
use level::Level;
use profile::Addresses;

const SCREENWIDTH: usize = 320;
const SCREENHEIGHT: usize = 200;

struct Settings {
    units_per_pixel: f32,
    wall_height: f32,
    fog: bool,
    mouse_sensitivity: f32,
    debug: bool,
    timing: bool,
    dump_prefix: Option<String>,
    dump_frames: Vec<u64>,
    dump_every: u64,
}

impl Settings {
    fn from_env() -> Settings {
        let env = |k: &str| std::env::var(k).ok();
        let num = |k: &str, default: f32| env(k).and_then(|v| v.parse().ok()).unwrap_or(default);
        Settings {
            units_per_pixel: num("DOOM2D_ZOOM", 2.0),
            wall_height: num("DOOM2D_WALL_HEIGHT", 32.0),
            fog: env("DOOM2D_FOG").is_none_or(|v| v != "0"),
            mouse_sensitivity: num("DOOM2D_MOUSE_SENS", 0.5),
            debug: env("DOOM2D_DEBUG").is_some(),
            timing: env("DOOM2D_PROFILE").is_some(),
            dump_prefix: env("DOOM2D_DUMP"),
            dump_frames: env("DOOM2D_DUMP_FRAMES")
                .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                .unwrap_or_default(),
            dump_every: env("DOOM2D_DUMP_EVERY").and_then(|v| v.parse().ok()).unwrap_or(0),
        }
    }
}

struct State {
    addr: Addresses,
    settings: Settings,
}

static STATE: OnceLock<State> = OnceLock::new();
/// Chargées au premier rendu ; `None` si le WAD est illisible.
static ASSETS: OnceLock<Option<Assets>> = OnceLock::new();
/// Dernier joueur rendu : sert à connaître son angle quand on construit la commande.
static PLAYER: AtomicPtr<Player> = AtomicPtr::new(core::ptr::null_mut());
static ORIG_BUILD_TICCMD: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicU64 = AtomicU64::new(0);

type BuildTiccmd = extern "C" fn(*mut Ticcmd, core::ffi::c_int);

#[used]
#[unsafe(link_section = ".init_array")]
static INIT: extern "C" fn() = init;

extern "C" fn init() {
    let Some(module) = hook::MainModule::find() else { return };
    // Pas Chocolate Doom (ou version inconnue) : on ne touche à rien, en silence.
    let Some((addr, source)) = Addresses::resolve(&module) else { return };

    let settings = Settings::from_env();
    input::set_sensitivity(settings.mouse_sensitivity);
    let render = addr.render_player_view;
    let slot = addr.build_ticcmd_slot() as *mut usize;
    let build = addr.build_ticcmd;
    let _ = STATE.set(State { addr, settings });

    eprintln!("[2dDOOM] adresses : {source}");
    if let Err(e) = unsafe { hook::install_jump(render, render_player_view as extern "C" fn(*mut Player) as usize) } {
        eprintln!("[2dDOOM] échec du détour du rendu : {e}");
        return;
    }
    // Crochet par pointeur : la fonction d'origine reste intacte et appelable.
    unsafe {
        if *slot == build {
            ORIG_BUILD_TICCMD.store(build, Ordering::Relaxed);
            *slot = build_ticcmd as BuildTiccmd as usize;
        } else {
            eprintln!("[2dDOOM] table de boucle inattendue, contrôles d'origine conservés");
        }
    }
}

/// Valeur d'une variable globale du jeu.
unsafe fn global<T: Copy>(addr: usize) -> T {
    unsafe { *(addr as *const T) }
}

/// Tableau du jeu (pointeur + nombre d'éléments), vide s'il n'est pas initialisé.
unsafe fn array<'a, T>(ptr: *const T, len: i32) -> &'a [T] {
    if ptr.is_null() || len <= 0 { &[] } else { unsafe { core::slice::from_raw_parts(ptr, len as usize) } }
}

/// Remplace `void G_BuildTiccmd(ticcmd_t *cmd, int maketic)`.
extern "C" fn build_ticcmd(cmd: *mut Ticcmd, maketic: core::ffi::c_int) {
    let orig: BuildTiccmd = unsafe { core::mem::transmute(ORIG_BUILD_TICCMD.load(Ordering::Relaxed)) };
    // Le jeu gère tout le reste : changer d'arme, menus, Espace…
    orig(cmd, maketic);

    let (Some(st), player) = (STATE.get(), PLAYER.load(Ordering::Relaxed)) else { return };
    if player.is_null() || cmd.is_null() {
        return;
    }
    let mo = unsafe { (*player).mo };
    let Some(controls) = input::read_controls() else { return };
    if mo.is_null() {
        return;
    }
    let cmd = unsafe { &mut *cmd };
    let mut target = input::aim_angle(input::cursor(), render::PIXEL_ASPECT);
    // « Utiliser » : on se tourne vers la porte ou l'interrupteur à portée le
    // temps d'un tic, quelle que soit la direction du viseur.
    if controls.use_ || cmd.buttons & game::BT_USE != 0 {
        let lines = unsafe { array::<Line>(global(st.addr.lines), global(st.addr.numlines)) };
        if let Some(angle) = unsafe { interact::use_target(lines, &*mo) } {
            target = angle;
        }
    }
    input::apply(cmd, controls, unsafe { (*mo).angle }, target);
    if st.settings.debug && (cmd.forwardmove != 0 || cmd.sidemove != 0 || cmd.buttons != 0) {
        eprintln!(
            "[2dDOOM] tic {maketic} : avant={} côté={} rotation={} boutons={:#x}",
            cmd.forwardmove, cmd.sidemove, cmd.angleturn, cmd.buttons
        );
    }
}

/// Remplace `void R_RenderPlayerView(player_t *player)`.
extern "C" fn render_player_view(player: *mut Player) {
    let Some(st) = STATE.get() else { return };
    let (a, s) = (&st.addr, &st.settings);
    let Some(assets) = ASSETS.get_or_init(|| {
        Assets::load().inspect_err(|e| eprintln!("[2dDOOM] ressources indisponibles : {e}")).ok()
    }) else {
        return;
    };
    unsafe {
        let video: *mut u8 = global(a.video_buffer);
        let flattranslation: *const i32 = global(a.flattranslation);
        let segs: *const Seg = global(a.segs);
        if video.is_null() || player.is_null() || flattranslation.is_null() || segs.is_null() {
            return;
        }
        let mut canvas = render::Canvas {
            buf: video,
            stride: SCREENWIDTH,
            x0: global(a.viewwindowx),
            y0: global(a.viewwindowy),
            w: global(a.scaledviewwidth),
            h: global(a.viewheight),
        };
        let frame = render::Frame {
            player: &*player,
            level: Level {
                lines: array::<Line>(global(a.lines), global(a.numlines)),
                nodes: array::<Node>(global(a.nodes), global(a.numnodes)),
                subsectors: array::<Subsector>(global(a.subsectors), global(a.numsubsectors)),
                segs,
            },
            sectors: array::<Sector>(global(a.sectors), global(a.numsectors)),
            assets,
            flattranslation,
            skyflatnum: global(a.skyflatnum),
            sprites: array::<SpriteDef>(global(a.sprites), global(a.numsprites)),
            units_per_pixel: s.units_per_pixel,
            wall_height: s.wall_height,
            fog: s.fog,
            timing: s.timing,
        };
        render::draw_frame(&mut canvas, &frame);
        input::set_view_size(canvas.w, canvas.h);
        render::crosshair(&mut canvas, input::cursor());
        PLAYER.store(player, Ordering::Relaxed);

        let n = FRAME.fetch_add(1, Ordering::Relaxed);
        if let Some(prefix) = &s.dump_prefix
            && (s.dump_frames.contains(&n) || (s.dump_every > 0 && n % s.dump_every == 0))
        {
            let screen = core::slice::from_raw_parts(video, SCREENWIDTH * SCREENHEIGHT);
            let path = format!("{prefix}-{n:05}.raw");
            if let Err(e) = std::fs::write(&path, screen) {
                eprintln!("[2dDOOM] capture {path} : {e}");
            }
        }
    }
}
