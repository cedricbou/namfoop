//! Ce qu'`export` et `import` partagent côté binaire : la mémoire des échanges de cette machine
//! (son dossier, son nom), l'heure, le dossier de la clé, et le bilan affiché famille par famille.
//! Le modèle de l'échange — inventaire, valise, comparaison — est dans `library::exchange`.
//!
//! La mémoire vit dans `echanges/`, **à côté** du fichier de config : `export` et `import` ne
//! lisent ni n'écrivent la config elle-même. Avec `--no-config`, il n'y a pas de dossier de config :
//! `--exchanges` en donne un.

use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

use crate::config::LoadedConfig;
use crate::theme;
use library::exchange::memory::{self, Machine};
use library::exchange::{Change, Element, Family, Group, Inventory};

/// Le dossier de la mémoire des échanges, à côté de la config.
pub const DIR: &str = "echanges";

/// Le dossier de la mémoire : `--exchanges`, sinon `echanges/` à côté du fichier de config.
pub fn memory_dir(option: Option<&Path>, conf: &LoadedConfig) -> Result<PathBuf> {
    if let Some(p) = option {
        return Ok(p.to_path_buf());
    }
    if conf.ignored {
        bail!(
            "--no-config : pas de dossier de config, donc pas de mémoire des échanges. \
             Donne-lui un dossier avec --exchanges <dossier>"
        );
    }
    Ok(conf.path.parent().unwrap_or(Path::new(".")).join(DIR))
}

/// Convertit une erreur de `library` (son `{:#}` garde la chaîne des causes).
pub fn lib_err(e: library::Error) -> anyhow::Error {
    anyhow!("{e:#}")
}

/// Le nom de cette machine. La première fois, il est demandé — le nom de l'ordinateur sur le
/// réseau est proposé. Il n'est retenu qu'avec l'échange (`remember`) : un import abandonné
/// n'écrit rien, pas même lui.
pub fn machine(dir: &Path, ui: &mut dyn Ui) -> Result<Machine> {
    if let Some(m) = memory::load_machine(dir).map_err(lib_err)? {
        return Ok(m);
    }
    let host = hostname();
    let answer = ui.line(&format!(" Comment s'appelle cet ordinateur ? [{host}] "));
    let name = answer.map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).unwrap_or(host);
    Ok(Machine { name, last: None })
}

/// Retient l'échange : l'état commun de la machine et de la valise, le dossier de la clé, et le
/// nom de la machine.
pub fn remember(
    dir: &Path,
    machine: &mut Machine,
    id: &str,
    folder: &Path,
    elements: Vec<Element>,
) -> Result<()> {
    let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.into());
    let x = memory::Exchange { valise: id.into(), folder, date: now(), elements };
    memory::save_exchange(dir, &x).map_err(lib_err)?;
    machine.last = Some(id.into());
    memory::save_machine(dir, machine).map_err(lib_err)
}

/// Les questions d'`export` et d'`import` : une ligne (le nom de la machine) ou une touche
/// (continuer, abandonner). Les tests répondent à la place du clavier.
pub trait Ui {
    /// Une ligne ; fin d'entrée : pas de réponse.
    fn line(&mut self, question: &str) -> Option<String>;
    /// Une touche, sans Entrée dans un terminal (`'\n'` pour Entrée) ; personne pour répondre :
    /// `None`.
    fn key(&mut self, question: &str) -> Option<char>;
}

/// Le clavier de l'utilisateur.
pub struct Terminal;

impl Ui for Terminal {
    fn line(&mut self, question: &str) -> Option<String> {
        use std::io::Write;
        eprint!("{question}");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    }

    fn key(&mut self, question: &str) -> Option<char> {
        use std::io::{IsTerminal, Write};
        eprint!("{question}");
        let _ = std::io::stderr().flush();
        if !std::io::stdin().is_terminal() {
            // un script : la première lettre de la ligne, Entrée pour une ligne vide
            let l = self.line("");
            eprintln!();
            return Some(l?.trim_end_matches(['\n', '\r']).chars().next().unwrap_or('\n'));
        }
        let c = read_key();
        eprintln!();
        c
    }
}

/// Une touche, le terminal sans écho ni mode ligne le temps de la lire, puis rendu tel quel.
fn read_key() -> Option<char> {
    // SAFETY : `termios` est une structure C simple, remplie par `tcgetattr` avant tout usage ;
    // la lecture est bloquante (VMIN = 1) dans un tampon local
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
            return None;
        }
        let saved = t;
        t.c_lflag &= !(libc::ICANON | libc::ECHO);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) != 0 {
            return None;
        }
        let mut buf = [0u8; 8];
        let n = libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len());
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
        if n <= 0 {
            return None;
        }
        let c = std::str::from_utf8(&buf[..n as usize]).ok()?.chars().next()?;
        Some(if c == '\r' { '\n' } else { c })
    }
}

/// Un autre namfoop joue-t-il sur cette machine (`browse`, `run`) ? Sa banque en mémoire ne
/// suivrait pas un import : on le cherche parmi les processus, sans verrou à laisser derrière soi
/// si namfoop s'arrête mal.
pub fn playing() -> bool {
    if cfg!(test) {
        return false;
    }
    let me = std::process::id().to_string();
    let Ok(rd) = std::fs::read_dir("/proc") else { return false };
    rd.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == me || !name.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let comm = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
        if comm.trim() != "namfoop" {
            return false;
        }
        let cmd = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        cmd.split(|b| *b == 0).any(|a| a == b"browse" || a == b"run" || a == b"--review")
    })
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: le tampon est valide et sa taille est passée ; gethostname termine la chaîne.
    let r = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    let n = buf.iter().position(|b| *b == 0).unwrap_or(0);
    let name = if r == 0 { String::from_utf8_lossy(&buf[..n]).into_owned() } else { String::new() };
    if name.is_empty() {
        "machine".into()
    } else {
        name
    }
}

/// L'heure locale, avec son décalage : celle que la machine croit, notée et jamais comparée.
pub fn now() -> toml::value::Datetime {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    // SAFETY: `tm` est initialisé par localtime_r, qui ne garde aucun pointeur.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    let off = tm.tm_gmtoff / 60;
    let text = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        if off < 0 { '-' } else { '+' },
        off.abs() / 60,
        off.abs() % 60
    );
    text.parse().unwrap_or_else(|_| "1970-01-01T00:00:00Z".parse().expect("date valide"))
}

/// Une graine pour l'identité d'une nouvelle valise.
pub fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (u64::from(std::process::id()) << 32)
}

/// « lun. 23/11 », et l'heure avec `time` : « mer. 25/11 22:10 ».
pub fn short_date(d: &toml::value::Datetime, time: bool) -> String {
    let Some(date) = d.date else { return d.to_string() };
    const DAYS: [&str; 7] = ["jeu.", "ven.", "sam.", "dim.", "lun.", "mar.", "mer."];
    let days = days_from_civil(i64::from(date.year), u32::from(date.month), u32::from(date.day));
    let mut s = format!("{} {:02}/{:02}", DAYS[days.rem_euclid(7) as usize], date.day, date.month);
    if let (true, Some(t)) = (time, d.time) {
        s.push_str(&format!(" {:02}:{:02}", t.hour, t.minute));
    }
    s
}

/// (année, mois, jour) -> jours depuis le 1970-01-01 (un jeudi), calendrier grégorien.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Le dossier de la clé : celui qu'on donne, sinon celui du dernier échange. Absent, le message
/// le nomme.
pub fn folder(arg: Option<&Path>, dir: &Path, m: &Machine) -> Result<PathBuf> {
    let f = match arg {
        Some(p) => p.to_path_buf(),
        None => {
            let last = m.last.as_deref().ok_or_else(|| {
                anyhow!("aucun échange retenu sur cette machine : donne le dossier de la clé")
            })?;
            memory::load_exchange(dir, last)
                .map_err(lib_err)?
                .ok_or_else(|| {
                    anyhow!("aucun échange retenu sur cette machine : donne le dossier de la clé")
                })?
                .folder
        }
    };
    if !f.is_dir() {
        let name =
            f.file_name().map_or_else(|| f.display().to_string(), |n| n.to_string_lossy().into_owned());
        bail!("clé {name} absente : branche-la, ou donne un dossier ({})", f.display());
    }
    Ok(f)
}

/// Des octets lisibles : « 44 Mo », « 1,2 Mo », « 830 Ko ».
pub fn size(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 10.0 {
        format!("{mb:.0} Mo")
    } else if mb >= 1.0 {
        format!("{mb:.1} Mo").replace('.', ",")
    } else {
        format!("{} Ko", bytes.div_ceil(1000))
    }
}

/// « 6 s », « 0,4 s ».
pub fn seconds(s: f64) -> String {
    if s >= 10.0 {
        format!("{s:.0} s")
    } else {
        format!("{s:.1} s").replace('.', ",")
    }
}

/// La couleur d'un changement : vert ce qui arrive, jaune ce qui change, rouge ce qui part.
pub fn tint(c: Change) -> ratatui::style::Color {
    match c {
        Change::Added => theme::ok(),
        Change::Modified => theme::warn(),
        Change::Removed => theme::danger(),
    }
}

/// Jusqu'à trois noms, puis « … et 5 autres ».
pub fn names(v: &[String]) -> String {
    let mut s = v.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if v.len() > 3 {
        s.push_str(&format!(" … et {} autres", v.len() - 3));
    }
    s
}

/// Ce qu'un état compte, en clair : « 312 sons, 16 presets, 3 sets ».
pub fn totals(inv: &Inventory) -> String {
    let sounds = inv.count(Family::Model) + inv.count(Family::Ir);
    format!(
        "{sounds} {}, {}, {}",
        if sounds > 1 { "sons" } else { "son" },
        Family::Preset.count(inv.count(Family::Preset)),
        Family::Set.count(inv.count(Family::Set))
    )
}

/// « + 2 modèles  + 1 IR  ~ 1 preset », en clair et peint, dans l'ordre des familles.
pub fn counts(items: impl Iterator<Item = (Change, Family)>) -> (String, String) {
    let mut counts: Vec<(Family, Change, usize)> = Vec::new();
    for (c, f) in items {
        match counts.iter_mut().find(|(g, k, _)| *g == f && *k == c) {
            Some(x) => x.2 += 1,
            None => counts.push((f, c, 1)),
        }
    }
    counts.sort();
    let texts: Vec<(String, Change)> =
        counts.iter().map(|(f, c, n)| (format!("{} {}", c.sign(), f.count(*n)), *c)).collect();
    let plain = texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join("  ");
    let painted = texts.iter().map(|(t, c)| theme::paint(t, tint(*c))).collect::<Vec<_>>().join("  ");
    (plain, painted)
}

/// Le bilan : une ligne par famille de l'échange (sons, créations, instruments) — ce qui change,
/// `+` `~` `-`, puis les noms ; ou `=` et le compte quand rien ne bouge.
pub fn report(changes: &[(Change, Element)], now: &Inventory) -> Vec<String> {
    const COUNTS: usize = 28;
    Group::ALL
        .iter()
        .map(|g| {
            let mine: Vec<&(Change, Element)> =
                changes.iter().filter(|(_, e)| e.kind.group() == *g).collect();
            let (plain, painted, names) = if mine.is_empty() {
                let parts: Vec<String> = Family::ALL
                    .iter()
                    .filter(|f| f.group() == *g)
                    .map(|f| (*f, now.count(*f)))
                    .filter(|(_, n)| *n > 0)
                    .map(|(f, n)| f.count(n))
                    .collect();
                let text = format!("= {}", if parts.is_empty() { "aucun".into() } else { parts.join(", ") });
                (text.clone(), theme::paint(&text, theme::dim()), String::new())
            } else {
                let (plain, painted) = counts(mine.iter().map(|(c, e)| (*c, e.kind)));
                (plain, painted, group_names(*g, &mine))
            };
            let pad = " ".repeat(COUNTS.saturating_sub(plain.chars().count()));
            format!(" {:<12} {painted}{pad}  {names}", g.label()).trim_end().to_string()
        })
        .collect()
}

/// Les noms de ce qui bouge : les packs et la taille pour les sons, les noms sinon.
fn group_names(g: Group, mine: &[&(Change, Element)]) -> String {
    if g != Group::Sounds {
        let v: Vec<String> = mine.iter().map(|(_, e)| e.name.clone()).collect();
        return names(&v);
    }
    let mut packs: Vec<String> = Vec::new();
    for (_, e) in mine {
        let p = e.pack().unwrap_or(&e.name).to_string();
        if !packs.contains(&p) {
            packs.push(p);
        }
    }
    let bytes: u64 = mine.iter().filter(|(c, _)| *c != Change::Removed).map(|(_, e)| e.size).sum();
    if bytes == 0 {
        names(&packs)
    } else {
        format!("{} · {}", names(&packs), size(bytes))
    }
}

/// Des réponses écrites d'avance, pour les tests : chaque question prend la suivante.
#[cfg(test)]
pub struct Script(pub std::collections::VecDeque<String>);

#[cfg(test)]
impl Script {
    pub fn new(answers: &[&str]) -> Self {
        Self(answers.iter().map(|a| a.to_string()).collect())
    }
}

#[cfg(test)]
impl Ui for Script {
    fn line(&mut self, _: &str) -> Option<String> {
        self.0.pop_front()
    }
    fn key(&mut self, _: &str) -> Option<char> {
        self.0.pop_front().map(|a| a.chars().next().unwrap_or('\n'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_in_french() {
        let d: toml::value::Datetime = "2026-11-25T22:10:00+01:00".parse().unwrap();
        assert_eq!(short_date(&d, true), "mer. 25/11 22:10");
        let d: toml::value::Datetime = "2026-11-23T08:00:00+01:00".parse().unwrap();
        assert_eq!(short_date(&d, false), "lun. 23/11");
        assert!(now().offset.is_some(), "l'heure porte son décalage");
    }

    #[test]
    fn sizes_and_names() {
        assert_eq!(size(44_000_000), "44 Mo");
        assert_eq!(size(1_234_567), "1,2 Mo");
        assert_eq!(size(830_000), "830 Ko");
        assert_eq!(seconds(6.04), "6,0 s");
        let v: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        assert_eq!(names(&v), "a, b, c … et 2 autres");
    }

    #[test]
    fn without_no_config_the_memory_sits_next_to_the_config() {
        let conf = LoadedConfig::load(Some("/x/namfoop/config.toml".into()), false).unwrap();
        assert_eq!(memory_dir(None, &conf).unwrap(), PathBuf::from("/x/namfoop/echanges"));
        let conf = LoadedConfig::load(None, true).unwrap();
        let e = memory_dir(None, &conf).unwrap_err().to_string();
        assert!(e.contains("--exchanges"), "{e}");
        assert_eq!(memory_dir(Some(Path::new("/m")), &conf).unwrap(), PathBuf::from("/m"));
    }
}
