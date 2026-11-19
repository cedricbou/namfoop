//! **Le set de concert**, côté exécution : écrire son fichier, lire ses prompteurs, ouvrir celui
//! du morceau dans `$EDITOR`, et retenir le set du moment dans la config (§ 4, § 7, § 8, § 10 de
//! `docs/design/set.md`).
//!
//! `browse::state::set` décide ; ici, on touche aux fichiers.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{Ctx, Flow};
use crate::browse::state::{self, State};
use crate::config::LoadedConfig;
use crate::tui;
use library::Library;

/// **`S` en édition de set** : le fichier, puis la banque, et la sortie si `q` attendait.
pub(super) fn save(ctx: &mut Ctx, stem: &str) -> Result<Flow> {
    let Some(set) = ctx.st.set.as_ref() else { return Ok(Flow::Next) };
    let file = Library::scan(ctx.library).set_path(stem);
    match set.spec.save(&file) {
        Ok(()) => {
            ctx.st.message = Some(format!("set « {stem} » enregistré ({})", file.display()));
            if let Some(set) = ctx.st.set.as_mut() {
                set.saved = set.spec.clone();
            }
            // un set tout juste créé (`L` › « ＋ nouveau set… ») n'avait pas d'identifiant : le
            // fichier existe maintenant, donc il en a un, et c'est à partir de là qu'on peut le
            // retenir dans `[set] default` (§ 4) — et que `run --set` sans argument le reprendra
            if ctx.st.set.as_ref().is_some_and(|x| x.id.is_empty()) {
                let id = Library::scan(ctx.library).by_path(&file).map(|e| e.id.clone()).unwrap_or_default();
                if !id.is_empty() {
                    if let Some(set) = ctx.st.set.as_mut() {
                        set.id = id.clone();
                    }
                    remember(ctx.conf, Some(&id));
                }
            }
            // le set a pu gagner ou perdre des presets : sa banque est à refaire
            ctx.rebuild_bank();
            // `S` depuis la fenêtre de `q` : on quitte maintenant que c'est écrit
            if std::mem::take(&mut ctx.st.quit_after_save) {
                return Ok(Flow::Quit);
            }
        }
        Err(e) => ctx.st.message = Some(format!("set « {stem} » non enregistré : {e:#}")),
    }
    Ok(Flow::Go)
}

/// **`t`** : le prompteur du morceau dans `$EDITOR` (§ 10). C'est ici que le terminal est rendu à
/// un autre programme ; l'état, lui, ne touche à aucun fichier.
pub(super) fn edit_prompt(ctx: &mut Ctx, song: usize) -> Result<Flow> {
    let what = ctx.st.set.as_ref().and_then(|set| {
        let s = set.spec.songs.get(song)?;
        Some((set.spec.base.join(s.prompt.as_deref()?), s.label().to_string()))
    });
    let Some((file, title)) = what else { return Ok(Flow::Next) };
    match open_editor(ctx.session, &file, &title) {
        Ok(()) => {
            ctx.st.adopt_prompts(read_prompt(&file));
            let lines = ctx.st.prompt_of(song).and_then(|l| l.prompt()).map(|p| p.line_count()).unwrap_or(0);
            ctx.st.message = Some(format!("prompteur « {title} » : {lines} ligne(s) — {}", file.display()));
        }
        Err(e) => ctx.st.message = Some(format!("prompteur « {title} » : {e:#}")),
    }
    Ok(Flow::Go)
}

/// **`L`** : la liste des sets de la bibliothèque, que seule cette frontière voit (§ 7).
pub(super) fn list(ctx: &mut Ctx) -> Result<Flow> {
    let sets = list_sets(ctx.library);
    ctx.st.adopt_sets(sets);
    Ok(Flow::Go)
}

/// Le set choisi : ses prompteurs, sa banque, et son premier son qui joue (§ 4, § 7).
pub(super) fn open(ctx: &mut Ctx, id: &str) -> Result<Flow> {
    match open_set(ctx.library, id, ctx.st) {
        Ok(set) => {
            ctx.st.message = Some(format!("set « {} » : {} morceaux", set.stem, set.spec.songs.len()));
            let prompts = read_prompts(&set.spec);
            ctx.st.adopt_prompts(prompts);
            ctx.st.set = Some(set);
            remember(ctx.conf, Some(id));
            // sa banque remplace la banque entière (§ 4)
            ctx.rebuild_bank();
            // et son premier son joue
            let first = ctx.st.set.as_ref().and_then(|s| s.flat().first().copied());
            if let Some(at) = first {
                ctx.st.play_set_sound(at);
            }
        }
        Err(e) => ctx.st.message = Some(format!("set « {id} » : {e:#}")),
    }
    Ok(Flow::Go)
}

/// Le set a changé de contenu à l'écran : plus de set du moment, et sa banque est à refaire.
pub(super) fn changed(ctx: &mut Ctx) -> Result<Flow> {
    // un set tout neuf n'a pas de dossier : ses prompteurs se résolvent dans `library/sets/`,
    // comme ceux d'un set lu sur le disque
    let dir = Library::scan(ctx.library).sets_dir();
    if let Some(set) = ctx.st.set.as_mut().filter(|s| s.spec.base.as_os_str().is_empty()) {
        set.spec.base = dir;
    }
    remember(ctx.conf, None);
    ctx.rebuild_bank();
    Ok(Flow::Go)
}

/// Les sets de la bibliothèque, tels que la fenêtre `L` les montre. Un set illisible y entre
/// **quand même**, marqué : c'est l'écran qui doit le dire, pas la liste qui doit le cacher.
pub(super) fn list_sets(library: &Path) -> Vec<state::set::screen::Entry> {
    let lib = Library::scan(library);
    lib.of_kind(library::Kind::Set)
        .map(|e| {
            let (name, songs, sounds) = match lib.set(&e.path) {
                Ok(spec) => (spec.name.clone(), spec.songs.len(), spec.sound_count()),
                Err(_) => (format!("⚠ {}", e.stem), 0, 0),
            };
            state::set::screen::Entry::Set {
                id: e.id.clone(),
                stem: e.stem.clone(),
                name,
                songs,
                sounds,
                rel: e.rel.clone(),
            }
        })
        .collect()
}

/// Les **prompteurs** d'un set, lus d'un coup à son ouverture : des fichiers texte de quelques
/// kilo-octets, et les lire ici évite toute lecture de fichier pendant un concert (§ 10).
pub(in crate::browse) fn read_prompts(
    spec: &library::set::SetSpec,
) -> Vec<(PathBuf, state::set::prompt::Loaded)> {
    spec.songs
        .iter()
        .filter_map(|s| s.prompt.as_deref())
        .flat_map(|rel| read_prompt(&spec.base.join(rel)))
        .collect()
}

/// Un prompteur. Un fichier absent ou illisible **n'est pas une erreur** : il est retenu comme tel,
/// le cadre le dit, et le concert continue (§ 10).
pub(super) fn read_prompt(path: &Path) -> Vec<(PathBuf, state::set::prompt::Loaded)> {
    use state::set::prompt::Loaded;
    let loaded = match library::prompt::Prompt::load(path) {
        Ok(p) => Loaded::Ok(p),
        Err(e) => Loaded::Failed(format!("{e:#}")),
    };
    vec![(path.to_path_buf(), loaded)]
}

/// **`$EDITOR` sur le `.cho`** (en le **créant** avec son titre s'il n'existe pas), puis retour à
/// l'écran. namfoop n'est pas l'éditeur du prompteur (§ 10) : ces fichiers viennent d'ailleurs.
fn open_editor(session: &mut tui::Session, file: &Path, title: &str) -> Result<()> {
    if !file.exists() {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, library::prompt::Prompt::skeleton(title))?;
    }
    let editor =
        std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".to_string());
    let mut parts = editor.split_whitespace();
    let prog = parts.next().unwrap_or("vi").to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();
    // l'éditeur a besoin du terminal **entier** : on quitte l'écran plein et le mode brut, puis
    // on y revient. On garde **le même** terminal : le recréer (`ratatui::init`) interroge le
    // terminal sur la position du curseur, et un terminal qui ne répond pas — un pseudo-terminal
    // de test, une console inhabituelle — ferait échouer le retour alors que tout va bien.
    use ratatui::crossterm::{execute, terminal};
    terminal::disable_raw_mode().context("sortie du mode brut")?;
    execute!(std::io::stdout(), terminal::LeaveAlternateScreen).context("sortie de l'écran plein")?;
    let status = std::process::Command::new(&prog).args(&args).arg(file).status();
    execute!(std::io::stdout(), terminal::EnterAlternateScreen).context("retour à l'écran plein")?;
    terminal::enable_raw_mode().context("retour au mode brut")?;
    // tout est à redessiner (l'écran plein a été quitté). **Pas** `Terminal::clear` : il demande
    // au terminal où est le curseur (`ESC [ 6 n`) et attend la réponse — un terminal qui ne
    // répond pas (un pseudo-terminal de test, une console inhabituelle) ferait échouer le retour
    // de l'éditeur alors que tout va bien. `resize` remet le tampon à zéro sans rien demander.
    let size = session.terminal.size().context("taille du terminal")?;
    session
        .terminal
        .resize(ratatui::layout::Rect::new(0, 0, size.width, size.height))
        .context("écran à redessiner")?;
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!("{prog} a quitté sans enregistrer ({s})"),
        Err(e) => bail!("{prog} : {e} (règle $EDITOR)"),
    }
}

/// Retient le set actif d'une session à l'autre (§ 4) : `[set] default` de la config, qui sert
/// aussi à `run --set` sans argument (§ 9). Sans config (`--no-config`), rien n'est écrit.
pub(super) fn remember(conf: &LoadedConfig, id: Option<&str>) {
    if conf.ignored {
        return;
    }
    let mut cfg = conf.config.clone();
    let mut set = cfg.set.clone().unwrap_or_default();
    if set.default.as_deref() == id {
        return;
    }
    set.default = id.map(str::to_string);
    cfg.set = Some(set);
    if let Err(e) = cfg.save(&conf.path) {
        log::warn!("set actif non retenu : {e:#}");
    }
}

/// Ouvre un set de la bibliothèque et place ses sons dans la banque de `browse` (§ 4).
/// Les presets sont retrouvés par identifiant **ou** par mots-clés, comme partout ailleurs.
pub(in crate::browse) fn open_set(library: &Path, query: &str, st: &State) -> Result<state::set::ActiveSet> {
    let lib = Library::scan(library);
    let path = lib.resolve_set(query)?;
    let spec = lib.set(&path)?;
    let id = lib.by_path(&path).map(|e| e.id.clone()).unwrap_or_default();
    let stem = library::set::display_name(&path);
    // un son du set désigne un preset : on retrouve sa place dans la banque de `browse`
    let find = |q: &str| {
        let p = lib.resolve_preset(q).ok()?;
        st.presets.iter().position(|x| x.path == p)
    };
    let set = state::set::ActiveSet::new(id, stem, spec, &find);
    for w in lib.set_warnings(&set.spec) {
        log::warn!("set {} : {}", set.stem, w.text);
    }
    Ok(set)
}
