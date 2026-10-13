//! topdoom : remplace le rendu 3D de Chocolate Doom par une vue du dessus.
//!
//! Chargé via `LD_PRELOAD`. Au chargement, on vérifie que le binaire est bien
//! celui du profil, puis on détourne `R_RenderPlayerView` vers notre rendu.
//!
//! Variables d'environnement :
//! - `TOPDOOM_ZOOM`        unités de carte par pixel (défaut 2)
//! - `TOPDOOM_WALL_HEIGHT` hauteur max des murs en relief, en unités (défaut 32)
//! - `TOPDOOM_FOG`         0 pour désactiver le brouillard de guerre
//! - `TOPDOOM_WAD`         WAD à lire (défaut : l'argument -iwad du jeu)
//! - `TOPDOOM_MOUSE_SENS`  pixels de viseur par pixel de souris (défaut 0.5)
//! - `TOPDOOM_DUMP`        préfixe de fichier pour capturer des images brutes
//! - `TOPDOOM_DUMP_FRAMES` numéros d'images à capturer, ex. "100,300"
//! - `TOPDOOM_DUMP_EVERY`  capturer une image toutes les N
//! - `TOPDOOM_DEBUG`       afficher les commandes envoyées au jeu
//! - `TOPDOOM_PROFILE`     afficher le temps passé dans chaque étape du rendu

mod assets;
mod floorcache;
mod game;
mod hook;
mod input;
mod level;
mod profile;
mod render;
mod visibility;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};

use assets::Assets;
use game::{Line, Node, Player, Sector, Seg, SpriteDef, Subsector, Ticcmd};
use level::Level;
use profile::Profile;

const SCREENWIDTH: usize = 320;
const SCREENHEIGHT: usize = 200;

struct State {
    base: usize,
    profile: &'static Profile,
    units_per_pixel: f32,
    wall_height: f32,
    fog: bool,
    dump_prefix: Option<String>,
    dump_frames: Vec<u64>,
    dump_every: u64,
    debug: bool,
    timing: bool,
}

static STATE: OnceLock<State> = OnceLock::new();
static FRAME: AtomicU64 = AtomicU64::new(0);
/// Chargées au premier rendu ; `None` si le WAD est illisible.
static ASSETS: OnceLock<Option<Assets>> = OnceLock::new();
/// Dernier joueur rendu : sert à connaître son angle au moment de construire la commande.
static PLAYER: AtomicPtr<Player> = AtomicPtr::new(core::ptr::null_mut());
/// G_BuildTiccmd d'origine, appelée avant nos modifications.
static ORIG_BUILD_TICCMD: AtomicUsize = AtomicUsize::new(0);

type BuildTiccmd = extern "C" fn(*mut Ticcmd, core::ffi::c_int);

#[used]
#[unsafe(link_section = ".init_array")]
static INIT: extern "C" fn() = init;

extern "C" fn init() {
    let profile = &profile::CHOCOLATE_DOOM_3_1_1;
    let Some(module) = hook::MainModule::find() else {
        return;
    };
    let base = module.base;
    let target = base + profile.render_player_view;
    if !module.code_matches(target, profile.render_signature) {
        // Pas le bon binaire (ou un autre processus) : on ne touche à rien, en silence.
        return;
    }

    let env = |k: &str| std::env::var(k).ok();
    let state = State {
        base,
        profile,
        units_per_pixel: env("TOPDOOM_ZOOM").and_then(|v| v.parse().ok()).unwrap_or(2.0),
        wall_height: env("TOPDOOM_WALL_HEIGHT").and_then(|v| v.parse().ok()).unwrap_or(32.0),
        fog: env("TOPDOOM_FOG").is_none_or(|v| v != "0"),
        dump_prefix: env("TOPDOOM_DUMP"),
        dump_frames: env("TOPDOOM_DUMP_FRAMES")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_default(),
        dump_every: env("TOPDOOM_DUMP_EVERY").and_then(|v| v.parse().ok()).unwrap_or(0),
        debug: env("TOPDOOM_DEBUG").is_some(),
        timing: env("TOPDOOM_PROFILE").is_some(),
    };
    let _ = STATE.set(state);
    input::set_sensitivity(env("TOPDOOM_MOUSE_SENS").and_then(|v| v.parse().ok()).unwrap_or(0.5));

    match unsafe { hook::install_jump(target, render_player_view as extern "C" fn(*mut Player) as usize) } {
        Ok(()) => eprintln!("[topdoom] {} : R_RenderPlayerView détourné à {target:#x}", profile.name),
        Err(e) => eprintln!("[topdoom] échec du détour : {e}"),
    }

    // Crochet par pointeur : on remplace l'entrée BuildTiccmd de la table de la
    // boucle principale. La fonction d'origine reste intacte et appelable.
    let slot = (base + profile.build_ticcmd_slot) as *mut usize;
    let expected = base + profile.build_ticcmd;
    unsafe {
        if *slot == expected {
            ORIG_BUILD_TICCMD.store(expected, Ordering::Relaxed);
            *slot = build_ticcmd as BuildTiccmd as usize;
            eprintln!("[topdoom] G_BuildTiccmd détourné (table à {:#x})", slot as usize);
        } else {
            eprintln!("[topdoom] table de boucle inattendue ({:#x}), contrôles d'origine conservés", *slot);
        }
    }
}

/// Remplace `void G_BuildTiccmd(ticcmd_t *cmd, int maketic)`.
extern "C" fn build_ticcmd(cmd: *mut Ticcmd, maketic: core::ffi::c_int) {
    let orig: BuildTiccmd = unsafe { core::mem::transmute(ORIG_BUILD_TICCMD.load(Ordering::Relaxed)) };
    // Le jeu gère tout le reste : utiliser, changer d'arme, menus…
    orig(cmd, maketic);

    let player = PLAYER.load(Ordering::Relaxed);
    if player.is_null() || cmd.is_null() {
        return;
    }
    let mo = unsafe { (*player).mo };
    let Some(controls) = input::read_controls() else { return };
    if mo.is_null() {
        return;
    }
    let mut target = input::aim_angle(input::cursor(), render::PIXEL_ASPECT);
    let cmd = unsafe { &mut *cmd };
    // « Utiliser » : on se tourne vers la porte ou l'interrupteur à portée,
    // le temps d'un tic, quelle que soit la direction du viseur.
    let wants_use = controls.use_ || cmd.buttons & game::BT_USE != 0;
    if wants_use && let Some(st) = STATE.get() {
        let lines = unsafe { array::<Line>(global(st, st.profile.lines), global(st, st.profile.numlines)) };
        let found = unsafe { use_target(lines, &*mo) };
        if st.debug {
            let (x, y) = unsafe { ((*mo).x >> 16, (*mo).y >> 16) };
            eprintln!("[topdoom] utiliser depuis ({x}, {y}) : cible {}", if found.is_some() { "trouvée" } else { "aucune à portée" });
        }
        if let Some(angle) = found {
            target = angle;
        }
    }
    input::apply(cmd, controls, unsafe { (*mo).angle }, target);
    if STATE.get().is_some_and(|s| s.debug) && (cmd.forwardmove != 0 || cmd.sidemove != 0 || cmd.buttons != 0) {
        eprintln!(
            "[topdoom] tic {maketic}: avant={} côté={} rotation={} boutons={:#x}",
            cmd.forwardmove, cmd.sidemove, cmd.angleturn, cmd.buttons
        );
    }
}

/// Portée de « utiliser » dans le jeu (USERANGE), un peu réduite par sécurité.
const USE_RANGE: f32 = 60.0;

/// Angle vers la ligne activable la plus proche, si elle est à portée.
///
/// Le jeu trace un rayon de 64 unités dans la direction du joueur et s'arrête au
/// premier mur : on vise donc un point de la ligne dégagé de tout obstacle.
/// Le joueur doit aussi être du côté avant de la ligne (règle de P_UseSpecialLine).
unsafe fn use_target(lines: &[Line], mo: &game::Mobj) -> Option<u32> {
    let f = |v: i32| v as f32 / 65536.0;
    let (px, py) = (f(mo.x), f(mo.y));
    let mut best: Option<(f32, (f32, f32))> = None;
    for (i, line) in lines.iter().enumerate() {
        if game::usable(line.special).is_none() {
            continue;
        }
        let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
        let (ax, ay) = (f(v1.x), f(v1.y));
        let (ex, ey) = (f(v2.x) - ax, f(v2.y) - ay);
        // Côté avant = à droite de v1 -> v2.
        if ex * (py - ay) - ey * (px - ax) > 0.0 {
            continue;
        }
        for k in 1..10 {
            let t = k as f32 / 10.0;
            let (qx, qy) = (ax + t * ex, ay + t * ey);
            let d = ((qx - px).powi(2) + (qy - py).powi(2)).sqrt();
            if d >= USE_RANGE || best.is_some_and(|(bd, _)| d >= bd) {
                continue;
            }
            let blocked = lines.iter().enumerate().any(|(j, other)| {
                j != i && visibility::blocks_sight(other) && unsafe { crosses(other, (px, py), (qx, qy)) }
            });
            if !blocked {
                best = Some((d, (qx - px, qy - py)));
            }
        }
    }
    let (_, (dx, dy)) = best?;
    Some((dy.atan2(dx) / core::f32::consts::TAU * 4294967296.0) as i64 as u32)
}

/// Le segment p -> q coupe-t-il la ligne ? (extrémités de q exclues)
unsafe fn crosses(line: &Line, p: (f32, f32), q: (f32, f32)) -> bool {
    let f = |v: i32| v as f32 / 65536.0;
    let (v1, v2) = unsafe { (&*line.v1, &*line.v2) };
    let (a, b) = ((f(v1.x), f(v1.y)), (f(v2.x), f(v2.y)));
    let cross = |o: (f32, f32), u: (f32, f32), w: (f32, f32)| (u.0 - o.0) * (w.1 - o.1) - (u.1 - o.1) * (w.0 - o.0);
    let (d1, d2) = (cross(a, b, p), cross(a, b, q));
    let (d3, d4) = (cross(p, q, a), cross(p, q, b));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) && d2.abs() > 1e-3
}

/// Tableau du jeu (pointeur + nombre d'éléments), vide si non initialisé.
unsafe fn array<'a, T>(ptr: *const T, len: i32) -> &'a [T] {
    if ptr.is_null() || len <= 0 { &[] } else { unsafe { core::slice::from_raw_parts(ptr, len as usize) } }
}

unsafe fn global<T: Copy>(state: &State, offset: usize) -> T {
    unsafe { *((state.base + offset) as *const T) }
}

/// Remplace `void R_RenderPlayerView(player_t *player)`.
extern "C" fn render_player_view(player: *mut Player) {
    let Some(st) = STATE.get() else { return };
    let p = st.profile;
    unsafe {
        let video: *mut u8 = global(st, p.video_buffer);
        if video.is_null() || player.is_null() {
            return;
        }
        let mut canvas = render::Canvas {
            buf: video,
            stride: SCREENWIDTH,
            x0: global(st, p.viewwindowx),
            y0: global(st, p.viewwindowy),
            w: global(st, p.scaledviewwidth),
            h: global(st, p.viewheight),
        };
        let assets = ASSETS.get_or_init(|| {
            Assets::load().inspect_err(|e| eprintln!("[topdoom] ressources indisponibles : {e}")).ok()
        });
        let Some(assets) = assets else { return };
        let frame = render::Frame {
            player: &*player,
            level: Level {
                lines: array(global(st, p.lines), global(st, p.numlines)),
                nodes: array::<Node>(global(st, p.nodes), global(st, p.numnodes)),
                subsectors: array::<Subsector>(global(st, p.subsectors), global(st, p.numsubsectors)),
                segs: global::<*const Seg>(st, p.segs),
            },
            sectors: array::<Sector>(global(st, p.sectors), global(st, p.numsectors)),
            assets,
            flattranslation: global(st, p.flattranslation),
            skyflatnum: global(st, p.skyflatnum),
            sprites: array::<SpriteDef>(global(st, p.sprites), global(st, p.numsprites)),
            units_per_pixel: st.units_per_pixel,
            wall_height: st.wall_height,
            fog: st.fog,
            timing: st.timing,
        };
        if frame.flattranslation.is_null() || frame.level.segs.is_null() {
            return;
        }
        render::draw_frame(&mut canvas, &frame);
        input::set_view_size(canvas.w, canvas.h);
        render::crosshair(&mut canvas, input::cursor());
        PLAYER.store(player, Ordering::Relaxed);

        let frame = FRAME.fetch_add(1, Ordering::Relaxed);
        if let Some(prefix) = &st.dump_prefix
            && (st.dump_frames.contains(&frame) || (st.dump_every > 0 && frame % st.dump_every == 0))
        {
            let screen = core::slice::from_raw_parts(video, SCREENWIDTH * SCREENHEIGHT);
            let path = format!("{prefix}-{frame:05}.raw");
            match std::fs::write(&path, screen) {
                Ok(()) => eprintln!("[topdoom] image {frame} -> {path}"),
                Err(e) => eprintln!("[topdoom] capture {path} : {e}"),
            }
        }
    }
}
