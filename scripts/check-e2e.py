#!/usr/bin/env python3
"""Harnais de bout en bout des écrans plein écran, dans un pseudo-terminal.

`browse` et `setup` prennent le terminal entier : leur logique est testée dans `browse::state` et
`setup::state` (pures, horloge fournie), mais la **boucle** — `browse/mod.rs`, 855 lignes, et
`browse/build.rs`, le thread qui décide de ce qu'on entend — n'était exercée par rien. Ce fichier
l'exerce de l'extérieur, comme un utilisateur.

Lancé par `scripts/check.sh`. Ne conclut pas si l'environnement ne permet pas d'ouvrir un
pseudo-terminal : il le dit et sort à 0, plutôt que de faire échouer une PR pour une raison qui
n'est pas le code.

Ce que ce harnais sait, et qui coûte une demi-journée à redécouvrir :

1. `os.kill(pid, 0)` RÉUSSIT sur un zombie : il ne dit pas si le processus a fini. Seul
   `waitpid(pid, WNOHANG)` le dit.
2. Il faut poser la taille du pseudo-terminal (`TIOCSWINSZ`), sinon ratatui ne dessine rien.
3. `Terminal::clear` de ratatui demande au terminal où est le curseur (`ESC [ 6 n`) et ATTEND la
   réponse ; dans un pseudo-terminal muet il échoue au bout de deux secondes. Ce harnais répond
   (`ESC [ 1 ; 1 R`), ce qui rend testable le retour de `$EDITOR`.
4. Un emoji occupe DEUX cellules du tampon ratatui : ne jamais coller une assertion à droite d'un
   emoji (« 🔒 SET » se lit « 🔒  SET »). `voir()` normalise les espaces pour cette raison.
5. Dans un écran à deux colonnes, chercher une ligne attrape les DEUX colonnes mêlées.
6. Ne pas scruter l'écran pour une valeur qui CHANGE : ratatui n'écrit que les cellules modifiées,
   si bien qu'une valeur arrive en morceaux épars. Pour « ce qu'on entend », la source de vérité est
   la trace (`NAMFOOP_BROWSE_TRACE`), pas l'écran.
"""

import os
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import time

RACINE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODELE = os.path.join(
    RACINE, "crates/nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam"
)


def sortir_sans_conclure(raison):
    print(f"  ⚠ bout en bout sauté : {raison}")
    sys.exit(0)


try:
    import fcntl
    import pty
    import termios
except ImportError as e:  # pragma: no cover - dépend de la plateforme
    sortir_sans_conclure(f"module absent ({e})")


class Ecran:
    """Un namfoop plein écran dans un pseudo-terminal."""

    def __init__(self, binaire, args, lignes, colonnes, trace, env=None):
        self.trace = trace
        self.tampon = ""
        maitre, esclave = pty.openpty()
        # piège 2 : sans TIOCSWINSZ, ratatui ne dessine rien du tout
        fcntl.ioctl(esclave, termios.TIOCSWINSZ, struct.pack("HHHH", lignes, colonnes, 0, 0))
        e = dict(os.environ, TERM="xterm-256color", NAMFOOP_BROWSE_TRACE=trace, RUST_LOG="off")
        e.update(env or {})
        self.proc = subprocess.Popen(
            [binaire, *args], stdin=esclave, stdout=esclave, stderr=esclave, env=e, close_fds=True
        )
        os.close(esclave)
        self.maitre = maitre

    def lire(self, duree=0.4):
        """Avale ce qui arrive pendant `duree`, en répondant aux demandes du terminal."""
        fin = time.monotonic() + duree
        while time.monotonic() < fin:
            pret, _, _ = select.select([self.maitre], [], [], 0.05)
            if not pret:
                continue
            try:
                bloc = os.read(self.maitre, 65536).decode("utf-8", "replace")
            except OSError:
                break
            self.tampon += bloc
            # piège 3 : répondre à « où est le curseur ? », sinon Terminal::clear échoue
            if "\x1b[6n" in bloc:
                os.write(self.maitre, b"\x1b[1;1R")
        return self.tampon

    def taper(self, touches, duree=0.4):
        os.write(self.maitre, touches.encode())
        return self.lire(duree)

    def voir(self):
        """L'écran en texte : séquences d'échappement retirées, espaces normalisés (piège 4)."""
        t = re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]", "", self.tampon)
        t = re.sub(r"\x1b[()][B0]|\x1b[=>]|\x1b\][^\x07]*\x07", "", t)
        return re.sub(r"[ \t]+", " ", t)

    def fini(self):
        """piège 1 : os.kill(pid, 0) réussit sur un zombie, waitpid est le seul juge."""
        pid, statut = os.waitpid(self.proc.pid, os.WNOHANG)
        return (os.waitstatus_to_exitcode(statut) if pid else None)

    def attendre_fin(self, duree=3.0):
        fin = time.monotonic() + duree
        while time.monotonic() < fin:
            self.lire(0.1)
            code = self.fini()
            if code is not None:
                return code
        return None

    def tuer(self):
        try:
            self.proc.kill()
            self.proc.wait(timeout=2)
        except Exception:
            pass
        try:
            os.close(self.maitre)
        except OSError:
            pass

    def lignes_de_trace(self):
        if not os.path.exists(self.trace):
            return []
        with open(self.trace, encoding="utf-8", errors="replace") as f:
            return [l for l in f.read().splitlines() if l.strip()]


def ir_de_synthese(chemin, secondes=0.1, taux=48000):
    """Une IR mono de 100 ms : bruit à décroissance exponentielle, graine fixe (même forme que
    `crates/dsp/tests/golden.rs`). Écrite à la main pour ne dépendre d'aucune bibliothèque."""
    n = int(secondes * taux)
    graine = 0x9E3779B9
    ech = []
    for i in range(n):
        graine ^= (graine << 13) & 0xFFFFFFFF
        graine ^= graine >> 17
        graine ^= (graine << 5) & 0xFFFFFFFF
        bruit = graine / 0xFFFFFFFF * 2.0 - 1.0
        ech.append(int(max(-1.0, min(1.0, bruit * pow(2.718281828, -i / (taux * 0.015)))) * 32767))
    donnees = struct.pack(f"<{n}h", *ech)
    entete = struct.pack(
        "<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(donnees), b"WAVE", b"fmt ", 16, 1, 1,
        taux, taux * 2, 2, 16, b"data", len(donnees),
    )
    with open(chemin, "wb") as f:
        f.write(entete + donnees)


def bibliotheque_jetable(dossier):
    """Une bibliothèque à usage unique : le modèle d'exemple du sous-module NAM et une IR de
    synthèse. Le modèle est marqué « IR requise », donc sans IR aucune chaîne ne s'installe et il
    n'y a rien à entendre — c'est le minimum pour que l'écran ait un son.

    Ne JAMAIS viser `library/` de l'utilisateur : `browse` y écrit (mélanges, presets, sets)."""
    nam = os.path.join(dossier, "nam")
    os.makedirs(nam, exist_ok=True)
    shutil.copy(MODELE, nam)
    for sous in ("ir/essai", "cabs", "presets", "sets"):
        os.makedirs(os.path.join(dossier, sous), exist_ok=True)
    ir_de_synthese(os.path.join(dossier, "ir/essai/essai.wav"))
    return dossier


ECHECS = []


def verifier(condition, message):
    if condition:
        print(f"  OK   {message}")
    else:
        print(f"  ÉCHEC {message}")
        ECHECS.append(message)


def scenario_browse(binaire, lignes, colonnes):
    """`browse` s'ouvre, dessine, fait jouer ce qu'on lui demande, et quitte sur `q`.

    La vérification qui compte porte sur la **trace** : elle dit quelle chaîne est entrée dans le
    thread audio. L'écran, lui, ne sert qu'à vérifier qu'on dessine — ratatui n'écrivant que les
    cellules modifiées, y chercher une valeur qui change retrouve l'ancienne (piège 6)."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-") as tmp:
        bib = bibliotheque_jetable(os.path.join(tmp, "library"))
        trace = os.path.join(tmp, "trace.log")
        ec = Ecran(
            binaire,
            # `--backend sim` : carte simulée, aucune sortie matérielle, donc rien d'audible —
            # c'est ce qui permet à un agent de lancer cet écran sans demander la carte.
            ["--no-config", "--backend", "sim", "--device", "4x6", "browse", "--library", bib],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(2.5)  # ouverture de la carte simulée, index de la bibliothèque, premier dessin
            vu = ec.voir()
            verifier(len(vu) > 500, f"{taille} : l'écran est dessiné ({len(vu)} caractères)")
            verifier("wavenet" in vu, f"{taille} : le modèle de la bibliothèque est listé")

            # ↓ : le curseur descend sur le modèle, et le banc d'essai le fait jouer AVEC son IR
            ec.taper("\x1b[B", 1.5)
            tr = ec.lignes_de_trace()
            derniere = tr[-1] if tr else ""
            verifier(
                "wavenet.nam" in derniere and "essai.wav" in derniere,
                f"{taille} : ↓ installe « modèle › IR » dans le thread audio",
            )

            # `b` coupe l'IR : la chaîne installée ne doit plus la contenir. C'est une promesse
            # visible par l'utilisateur, vérifiée là où elle compte — ce qui joue vraiment.
            avant = len(tr)
            ec.taper("b", 1.5)
            tr = ec.lignes_de_trace()
            verifier(len(tr) > avant, f"{taille} : `b` installe une nouvelle chaîne")
            if len(tr) > avant:
                derniere = tr[-1]
                verifier(
                    "wavenet.nam" in derniere and "essai.wav" not in derniere,
                    f"{taille} : `b` coupe l'IR et garde le modèle",
                )

            # La chaîne est vide alors que le banc joue : `q` doit AVERTIR au lieu de quitter
            # (`empty_chain_warning`) — on ne quitte pas par accident en ayant cru garder un son.
            ec.taper("q", 0.8)
            verifier(
                ec.fini() is None,
                f"{taille} : `q` avertit au lieu de quitter (chaîne vide, banc qui joue)",
            )
            # `Q` quitte quand même.
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : `Q` quitte proprement (code {code})")
        finally:
            ec.tuer()


def main():
    # NAMFOOP_BIN d'abord (check.sh le pose : la CI compile dans target/ci/)
    candidats = [os.environ["NAMFOOP_BIN"]] if os.environ.get("NAMFOOP_BIN") else []
    candidats += [os.path.join(RACINE, f"target/{p}/namfoop") for p in ("release", "ci", "debug")]
    binaire = next((c for c in candidats if os.path.exists(c)), None)
    if binaire is None:
        sortir_sans_conclure("aucun binaire compilé (cargo build)")
    if not os.path.exists(MODELE):
        sortir_sans_conclure("modèle d'exemple absent (git submodule update --init --recursive)")

    # les deux tailles que le projet utilise : le minimum, et l'écran 1280×800 du Pi
    for lignes, colonnes in ((25, 80), (50, 160)):
        scenario_browse(binaire, lignes, colonnes)

    if ECHECS:
        print(f"bout en bout : ÉCHEC ({len(ECHECS)})")
        sys.exit(1)
    print("bout en bout : OK")


if __name__ == "__main__":
    main()
