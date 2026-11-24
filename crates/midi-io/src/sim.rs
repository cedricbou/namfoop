//! Le **port simulé** : `sim:<chemin>`, une ligne de texte par message (format dans `text.rs`).
//! Un agent ne branche pas de pédalier, et l'utilisateur n'en a pas toujours un sous la main.
//!
//! Le chemin est un fichier ordinaire — la séquence est rejouée au démarrage, dans l'ordre — ou un
//! **tube nommé** (`mkfifo`), et c'est le cas intéressant : on envoie les messages un par un,
//! pendant que namfoop joue, comme on appuierait sur un pédalier.
//!
//! ```sh
//! mkfifo /tmp/m && namfoop --midi-in sim:/tmp/m browse &
//! echo 'pc 3' > /tmp/m
//! ```
//!
//! Le fichier est ouvert **sans bloquer** (`O_NONBLOCK`) : un tube sans écrivain bloquerait
//! l'ouverture, et le fil de lecture ne verrait plus l'ordre de s'arrêter. La boucle relit donc à
//! petits pas, ce qui lit aussi la suite d'un fichier qu'on complète (comme `tail -f`), et ne perd
//! pas une ligne coupée en deux par deux écritures.

use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{bail, Context, Result};
use crate::{text, Sink, Source};

/// Pas de la boucle de lecture : assez fin pour qu'une horloge tapée à la main garde son rythme,
/// assez gros pour ne rien coûter.
const PAS: Duration = Duration::from_millis(1);

/// Est-ce un port simulé plutôt qu'un port du séquenceur ?
pub(crate) fn is_sim(port: &str) -> bool {
    port.starts_with("sim:")
}

pub(crate) struct SimPort {
    path: PathBuf,
    file: std::fs::File,
}

impl SimPort {
    pub(crate) fn open(port: &str) -> Result<Self> {
        let path = PathBuf::from(port.trim_start_matches("sim:"));
        if path.as_os_str().is_empty() {
            bail!("port simulé : donner un chemin (ex. « sim:/tmp/m », créé par « mkfifo /tmp/m »)");
        }
        let file =
            std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path).context(
                format!(
                    "port MIDI simulé « {} » : fichier ou tube absent (« mkfifo {} » pour le créer)",
                    path.display(),
                    path.display()
                ),
            )?;
        Ok(Self { path, file })
    }

    pub(crate) fn description(&self) -> String {
        format!("port simulé {}", self.path.display())
    }
}

impl Source for SimPort {
    fn run(self: Box<Self>, mut out: Sink) {
        let mut file = self.file;
        let mut reste = String::new();
        let mut tampon = [0u8; 1024];
        while out.running() {
            // `Ok(0)` : fin du fichier, ou tube dont l'écrivain a fini. Ce n'est pas la fin de
            // l'écoute (on peut réécrire dans le tube), mais c'est le seul moment où l'on sait
            // qu'une dernière ligne sans retour chariot ne sera pas complétée : on la prend.
            let fin = match file.read(&mut tampon) {
                Ok(0) => true,
                Ok(n) => {
                    reste.push_str(&String::from_utf8_lossy(&tampon[..n]));
                    false
                }
                // rien à lire, ou un signal reçu pendant la lecture : on repasse, on n'abandonne
                // pas — une écoute qui s'arrête en silence ne se diagnostique pas
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    false
                }
                Err(e) => {
                    log::warn!("port MIDI simulé {} : lecture interrompue ({e})", self.path.display());
                    return;
                }
            };
            while let Some(i) = reste.find('\n') {
                let ligne: String = reste.drain(..=i).collect();
                lire(&ligne, &self.path, &mut out);
            }
            if fin && !reste.trim().is_empty() {
                let ligne = std::mem::take(&mut reste);
                lire(&ligne, &self.path, &mut out);
            }
            std::thread::sleep(PAS);
        }
    }
}

/// Une ligne fausse est **signalée et sautée** : un tube qu'on remplit à la main se trompe, et ce
/// n'est pas une raison d'arrêter d'écouter.
fn lire(ligne: &str, path: &Path, out: &mut Sink) {
    match text::parse_line(ligne) {
        Ok(Some((event, channel))) => out.push(event, channel),
        Ok(None) => {}
        Err(e) => log::warn!("port MIDI simulé {} : {e}", path.display()),
    }
}
