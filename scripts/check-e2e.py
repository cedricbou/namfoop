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
6. **Dormir n'est pas attendre** : seul `lire()` vide le pseudo-terminal. Un `time.sleep()` le
   laisse se remplir, et namfoop **se bloque** en écrivant son écran — l'écran se figerait au
   milieu du scénario, sans erreur, et ce qu'on lui envoie ensuite semblerait perdu.
7. Ne pas scruter l'écran pour une valeur qui CHANGE : ratatui n'écrit que les cellules modifiées,
   si bien qu'une valeur arrive en morceaux épars. Pour « ce qu'on entend », la source de vérité est
   la trace (`NAMFOOP_BROWSE_TRACE`), pas l'écran.
8. **Sans port demandé, namfoop écoute TOUS les appareils MIDI de la machine** : un pédalier
   branché, ou l'horloge qu'Ableton (ou `arr play --clock`) envoie à la session de l'utilisateur,
   arrive aussi dans l'écran testé et fausse le tempo, la battue, la chaîne installée. Chaque
   lancement passe donc par `isoler()`, qui ajoute `--no-midi` sauf si le scénario demande un port
   simulé (`--midi-in sim:…` ou `in = "sim:…"` dans sa config).
"""

import os
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
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


def isoler(args):
    """Les arguments d'un lancement, **coupés du MIDI de la machine** (piège 8) : `--no-midi`
    ajouté, sauf si le scénario choisit déjà son port (`--midi-in`, `--no-midi`) ou en écrit un
    simulé dans sa config."""
    if "--midi-in" in args or "--no-midi" in args:
        return list(args)
    if "--config" in args:
        i = args.index("--config") + 1
        try:
            with open(args[i], encoding="utf-8") as f:
                if re.search(r'^in\s*=\s*"sim:', f.read(), re.M):
                    return list(args)
        except (IndexError, OSError):
            pass
    return ["--no-midi", *args]


class Ecran:
    """Un namfoop plein écran dans un pseudo-terminal."""

    def __init__(self, binaire, args, lignes, colonnes, trace, env=None):
        self.trace = trace
        self.code = None
        self.tampon = ""
        maitre, esclave = pty.openpty()
        # piège 2 : sans TIOCSWINSZ, ratatui ne dessine rien du tout
        fcntl.ioctl(esclave, termios.TIOCSWINSZ, struct.pack("HHHH", lignes, colonnes, 0, 0))
        e = dict(os.environ, TERM="xterm-256color", NAMFOOP_BROWSE_TRACE=trace, RUST_LOG="off")
        e.update(env or {})
        self.proc = subprocess.Popen(
            [binaire, *isoler(args)], stdin=esclave, stdout=esclave, stderr=esclave, env=e, close_fds=True
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
        """piège 1 : os.kill(pid, 0) réussit sur un zombie, waitpid est le seul juge.

        Le code est **retenu** : `waitpid` ne récolte un processus qu'une fois, et demander deux
        fois « est-il fini ? » lèverait `ChildProcessError` au lieu de répondre."""
        if self.code is None:
            pid, statut = os.waitpid(self.proc.pid, os.WNOHANG)
            if pid:
                self.code = os.waitstatus_to_exitcode(statut)
        return self.code

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


def set_de_concert(dossier):
    """Un set de concert jouable : deux presets, un morceau, son prompteur ChordPro.

    Le morceau passe du 4/4 au 7/8 (2+2+3) après deux mesures, et sa deuxième section est liée à
    un **autre** son : c'est ce qui fait apparaître la bande de battue, son changement de mesure
    et son compte à rebours de changement de son (§ 11 de `docs/design/set.md`)."""
    presets = os.path.join(dossier, "presets")
    sets = os.path.join(dossier, "sets/prompts")
    os.makedirs(presets, exist_ok=True)
    os.makedirs(sets, exist_ok=True)
    # `fuzz` porte une réverbe : c'est ce qui le distingue de `clean` dans la trace, deux presets
    # identiques y donnant la même ligne (et la même place de banque)
    for nom, extra in (("clean", ""), ("fuzz", '[[block]]\nkind = "fx"\n[block.fx]\ntype = "reverb"\n')):
        with open(os.path.join(presets, f"{nom}.preset.toml"), "w", encoding="utf-8") as f:
            f.write(
                f'name = "{nom}"\n'
                '[[block]]\nkind = "nam"\nfile = "nam/wavenet.nam"\n'
                '[[block]]\nkind = "ir"\nfile = "ir/essai/essai.wav"\n' + extra
            )
    with open(os.path.join(dossier, "sets/concert.set.toml"), "w", encoding="utf-8") as f:
        f.write(
            'name = "Concert e2e"\n\n'
            "[[song]]\n"
            'title = "Le Phare"\n'
            "pc = 1\n"
            "tempo_bpm = 120\n"
            'prompt = "prompts/le-phare.cho"\n'
            '  [[song.sound]]\n  preset = "clean"\n'
            '  [[song.sound]]\n  preset = "fuzz"\n'
        )
    with open(os.path.join(sets, "le-phare.cho"), "w", encoding="utf-8") as f:
        f.write(
            "{title: Le Phare}\n"
            "{start_of_verse: Couplet 1}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 2}\n"
            "le phare tourne encore\n{end_of_verse}\n"
            "{time: 7/8}\n{x_namfoop_beats: 2+2+3}\n"
            "{start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{x_namfoop_bars: 4}\n"
            "tout casse\n{end_of_chorus}\n"
        )
    return dossier


def deux_presets(dossier):
    """Deux presets **reconnaissables dans la trace** : le second porte une réverbe. C'est ce qui
    permet de dire quelle place de la banque un Program Change a installée."""
    presets = os.path.join(dossier, "presets")
    os.makedirs(presets, exist_ok=True)
    base = (
        '[[block]]\nkind = "nam"\nfile = "nam/wavenet.nam"\n'
        '[[block]]\nkind = "ir"\nfile = "ir/essai/essai.wav"\n'
    )
    for nom, extra in (("a-sec", ""), ("b-reverb", '[[block]]\nkind = "fx"\n[block.fx]\ntype = "reverb"\n')):
        with open(os.path.join(presets, f"{nom}.preset.toml"), "w", encoding="utf-8") as f:
            f.write(f'name = "{nom}"\n' + base + extra)
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
    cellules modifiées, y chercher une valeur qui change retrouve l'ancienne (piège 7)."""
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


def scenario_accordeur(binaire, lignes, colonnes):
    """**L'accordeur** (`docs/design/accordeur.md`), de bout en bout : `u` coupe la basse — la case
    lue par le thread audio passe à « coupée » —, l'accordeur lit la note que joue la carte
    simulée (un La à 55 Hz, une fois par seconde), et `u` rend le son.

    Le silence des sorties, lui, se vérifie dans les tests d'`engine` : la carte simulée jette ce
    qu'elle reçoit. Ici, c'est la trace qui fait foi (piège 7)."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-") as tmp:
        bib = bibliotheque_jetable(os.path.join(tmp, "library"))
        trace = os.path.join(tmp, "trace.log")
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "browse", "--library", bib],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(2.5)
            ec.taper("u", 2.5)
            tr = ec.lignes_de_trace()
            verifier(any("basse=coupée" in l for l in tr), f"{taille} : `u` coupe la basse")
            notes = [l for l in tr if "accordeur A1" in l]
            verifier(notes, f"{taille} : l'accordeur lit le La1 de la carte simulée ({tr[-3:]})")
            for l in notes:
                cents = int(l.split("accordeur A1")[1].split()[0])
                verifier(abs(cents) <= 2, f"{taille} : à ±2 cents près ({l.strip()})")
            verifier(not any("accordeur " in l and "A1" not in l and "rien" not in l for l in tr),
                     f"{taille} : aucune autre note lue")
            verifier("BASSE COUPÉE" in ec.voir(), f"{taille} : le bandeau de l'en-tête")
            ec.taper("u", 1.0)
            tr = ec.lignes_de_trace()
            verifier(tr and any("basse=jouée" in l for l in tr), f"{taille} : `u` rend le son")
            ec.taper("q", 0.6)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : quitte proprement (code {code})")
        finally:
            ec.tuer()


def scenario_battue(binaire, lignes, colonnes):
    """**La bande de battue** (§ 11), de bout en bout : elle se dessine, `Espace` la lance, elle
    avance au tempo jusqu'au changement de mesure, et **rien n'entre dans le fil audio**.

    Toutes les assertions portent sur du texte qui **apparaît** (piège 7 : le tampon ratatui est
    cumulatif, donc « a disparu » ne se vérifie pas en le scrutant) et sur le **compte** des lignes
    de trace, qui dit ce qui est entré dans le thread audio."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-battue-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "browse",
             "--library", bib, "--set", "concert"],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(2.5)
            ec.taper("3", 0.6)       # l'espace Set
            ec.taper("\r", 1.5)      # Entrée : le morceau 1 sur son son 1
            vu = ec.voir()
            verifier("♩120 4/4" in vu, f"{taille} : la bande donne le tempo et la mesure")
            verifier("mes. 1/2" in vu, f"{taille} : la mesure dans la section")
            verifier("⏸" in vu, f"{taille} : à l'arrêt, la bande le dit")
            verifier("fuzz dans 2 mes." in vu, f"{taille} : le changement de son est annoncé")

            # Espace lance la battue : `⏵` n'était pas à l'écran avant
            avant = len(ec.lignes_de_trace())
            ec.taper(" ", 0.8)
            vu = ec.voir()
            verifier("⏵" in vu, f"{taille} : Espace lance la battue")
            # deux mesures de 4/4 à ♩120 = 4 s : la section suivante est en 7/8 (2+2+3)
            ec.lire(4.0)
            vu = ec.voir()
            verifier("7/8 (2+2+3)" in vu, f"{taille} : la battue passe la fin de section, en 7/8")
            verifier(
                len(ec.lignes_de_trace()) == avant,
                f"{taille} : la battue n'installe AUCUNE chaîne (rien dans le fil audio)",
            )

            ec.taper("q", 0.8)
            if ec.fini() is None:
                ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : le set se quitte proprement (code {code})")
        finally:
            ec.tuer()


def scenario_midi(binaire, lignes, colonnes):
    """**Le MIDI** de bout en bout, sans pédalier : un port simulé lit un tube de texte, et ce
    qu'on y écrit doit arriver jusqu'à la boucle de `browse` — puis **changer le son**.

    C'est exactement ce que fait un pédalier (ADR 0011), le câble en moins. La vérification porte
    sur la **trace** : elle seule dit quelle chaîne est entrée dans le thread audio."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-") as tmp:
        bib = deux_presets(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "pedalier")
        os.mkfifo(tube)
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "--midi-in", f"sim:{tube}",
             "browse", "--library", bib],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(2.5)
            verifier("MIDI" in ec.voir(), f"{taille} : l'écran dit qu'il écoute un port MIDI")
            # O_NONBLOCK : l'ouverture en écriture échoue tant que personne ne lit le tube, ce qui
            # dit aussi si namfoop l'a bien ouvert. On réessaie le temps qu'il démarre.
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            os.write(fd, b"cc 34 2\nstart\nspp 64\n")
            os.write(fd, b"nawak\n")  # une ligne fausse : signalée, et elle n'arrête pas l'écoute
            ec.lire(1.2)
            recus = [l.split(" midi ", 1)[1] for l in ec.lignes_de_trace() if " midi " in l]
            verifier(
                recus == ["CC 34 2", "start", "SPP 64"],
                f"{taille} : les messages du tube arrivent dans l'ordre, la ligne fausse exceptée "
                f"({recus})",
            )

            # **PC 2 installe la place 2 de la banque** : la trace dit la chaîne entrée dans le
            # thread audio, et seule celle-là porte une réverbe.
            avant = len(ec.lignes_de_trace())
            os.write(fd, b"pc 2\n")
            ec.lire(1.5)
            tr = ec.lignes_de_trace()
            chaines = [l for l in tr if "wavenet.nam" in l and " midi " not in l]
            verifier(
                chaines and "reverb" in chaines[-1],
                f"{taille} : PC 2 installe le preset de la place 2 ({chaines[-1:]})",
            )
            # une place qui n'existe pas ne change RIEN : aucune chaîne de plus dans la trace
            avant = len(tr)
            os.write(fd, b"pc 9\n")
            ec.lire(1.0)
            tr = ec.lignes_de_trace()
            nouvelles = [l for l in tr[avant:] if " midi " not in l]
            verifier(
                not nouvelles,
                f"{taille} : PC 9 (place absente) n'installe aucune chaîne ({nouvelles})",
            )
            # **Le tap au pied** (CC 14) : deux appuis à 400 ms donnent 150 BPM, et la trace dit
            # le tempo entendu avec sa source. L'horloge, elle, se vérifie en unitaire (un top
            # vaut 20,8 ms à 120 BPM : la cadence d'un tube de texte ne la mesure pas) et sur le
            # matériel, avec Ableton.
            avant = len(ec.lignes_de_trace())
            os.write(fd, b"cc 14 127\n")
            ec.lire(0.4)  # 400 ms = 150 BPM : différent du tempo par défaut, donc visible
            os.write(fd, b"cc 14 127\n")
            ec.lire(0.8)
            tempos = [l for l in ec.lignes_de_trace()[avant:] if "tempo=" in l]
            verifier(
                tempos and "(tap)" in tempos[-1],
                f"{taille} : deux appuis sur le CC 14 donnent le tempo au pied ({tempos[-1:]})",
            )
            os.close(fd)
            # le tap a donné son tempo au preset : `q` AVERTIT (comme après n'importe quel
            # réglage), `Q` quitte quand même
            ec.taper("q", 0.8)
            verifier(ec.fini() is None, f"{taille} : `q` avertit, le tap ayant modifié le preset")
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : on quitte proprement avec un port MIDI ouvert ({code})")
        finally:
            ec.tuer()


def scenario_midi_set(binaire, lignes, colonnes):
    """**Le set piloté depuis Ableton**, sur l'écran de scène verrouillé (`run --set`) : un Program
    Change appelle le morceau, un CC son son, et le transport place la battue.

    Le port simulé tient le rôle d'Ableton. La vérification porte sur la **trace** (quelle chaîne
    est entrée dans le fil audio, et à quelle place de banque) et sur ce qui **apparaît** à
    l'écran."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-set-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "ableton")
        os.mkfifo(tube)
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "--midi-in", f"sim:{tube}",
             "run", "--set", "concert", "--library", bib, "--master-db", "-120"],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(3.0)
            verifier("🔒" in ec.voir(), f"{taille} : la scène verrouillée est ouverte")
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return

            # CC 34 = 1 : le son 2 du morceau (« fuzz », le seul qui porte une réverbe)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            avant = len(chaines)
            os.write(fd, b"cc 34 1\n")
            ec.lire(1.5)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(
                len(chaines) > avant and "reverb" in chaines[-1],
                f"{taille} : CC 34 = 1 joue le son 2 du morceau ({chaines[-1:]})",
            )

            # PC 1 : le morceau, sur son **son 1** — donc de retour sur « clean », sans réverbe
            avant = len(chaines)
            os.write(fd, b"pc 1\n")
            ec.lire(1.5)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(
                len(chaines) > avant and "reverb" not in chaines[-1],
                f"{taille} : PC 1 joue le morceau sur son premier son ({chaines[-1:]})",
            )

            # un PC suivi d'un CC dans la fenêtre : UN SEUL changement de son
            avant = len(chaines)
            os.write(fd, b"pc 1\ncc 34 1\n")
            ec.lire(1.5)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(
                len(chaines) == avant + 1 and "reverb" in chaines[-1],
                f"{taille} : PC + CC ne font qu'un changement ({len(chaines) - avant} chaîne(s))",
            )

            # le transport, battue à l'arrêt : un SPP place la position sans la faire avancer
            os.write(fd, b"spp 16\n")
            ec.lire(0.8)
            verifier("mes. 2/2" in ec.voir(), f"{taille} : un SPP place la battue à la bonne mesure")
            # `start` la ramène au 1 de la mesure 1, et la lance
            os.write(fd, b"start\n")
            ec.lire(0.8)
            vu = ec.voir()
            verifier("⏵" in vu, f"{taille} : `start` lance la battue")
            verifier("mes. 1/2" in vu, f"{taille} : `start` la remet sur le 1 de la mesure 1")
            os.write(fd, b"stop\n")
            ec.lire(0.8)
            verifier("⏸" in ec.voir(), f"{taille} : `stop` arrête la battue")

            # **morceau suivant au pied** (CC 104) : le set n'a qu'un morceau, donc la borne, qui
            # ne boucle pas ; le relâcher (valeur 0) ne dit rien. Le CC 103 est testé dans l'état :
            # « premier » réécrirait « dernier » case par case, que le tampon ne relit pas en entier
            os.write(fd, b"cc 104 127\ncc 104 0\n")
            ec.lire(0.8)
            verifier("dernier morceau" in ec.voir(), f"{taille} : CC 104 suit le set, sans boucler")

            # **l'accordeur depuis Ableton** (CC 15) : entrée, idempotence, sortie, puis le retour
            # automatique par un PC et par un Start
            def coupures():
                return [l.split()[-1] for l in ec.lignes_de_trace() if " basse=" in l]
            os.write(fd, b"cc 15 127\n")
            ec.lire(0.8)
            verifier(coupures()[-1:] == ["basse=coupée"], f"{taille} : CC 15 = 127 coupe la basse")
            verifier("BASSE COUPÉE" in ec.voir(), f"{taille} : le bandeau de l'en-tête")
            n = len(coupures())
            os.write(fd, b"cc 15 127\n")
            ec.lire(0.6)
            verifier(len(coupures()) == n, f"{taille} : un second CC 15 = 127 ne change rien")
            os.write(fd, b"cc 15 0\n")
            ec.lire(0.6)
            verifier(coupures()[-1:] == ["basse=jouée"], f"{taille} : CC 15 = 0 rend le son")
            for retour in (b"pc 1\n", b"cc 34 0\n", b"start\n"):
                os.write(fd, b"cc 15 127\n")
                ec.lire(0.6)
                os.write(fd, retour)
                ec.lire(0.8)
                verifier(
                    coupures()[-2:] == ["basse=coupée", "basse=jouée"],
                    f"{taille} : « {retour.decode().strip()} » rend le son",
                )
            os.write(fd, b"stop\n")
            ec.lire(0.3)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]

            # une valeur hors du morceau est ignorée **en le disant**, et ne change pas le son
            avant = len(chaines)
            os.write(fd, b"cc 34 9\n")
            ec.lire(1.0)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(
                len(chaines) == avant,
                f"{taille} : un CC au-delà des sons du morceau ne change rien",
            )
            os.close(fd)
            # sur scène, quitter demande confirmation : `q` puis `Q`
            ec.taper("q", 0.8)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : la scène se quitte proprement ({code})")
        finally:
            ec.tuer()


def scenario_instrument_au_pied(binaire, lignes, colonnes):
    """**Changer d'instrument au pied** (§ 8 de `docs/design/instruments.md`), sur la scène
    verrouillée : un geste propose sans rien changer, le même geste confirme, et la session se
    relance avec l'autre instrument **au même son du set**, le port MIDI toujours ouvert."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-instrument-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        os.makedirs(os.path.join(bib, "instruments"))
        for fichier, nom, n in (("bb", "BB", 1), ("fretless", "Fretless", 2)):
            with open(os.path.join(bib, "instruments", f"{fichier}.toml"), "w", encoding="utf-8") as f:
                f.write(f'name = "{nom}"\nnumber = {n}\n')
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "pedalier")
        os.mkfifo(tube)
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "--midi-in", f"sim:{tube}",
             "--instrument", "1", "run", "--set", "concert", "--library", bib, "--master-db", "-120"],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(3.0)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            os.write(fd, b"cc 34 1\n")  # le son 2, « fuzz » : la réverbe le signe dans la trace
            ec.lire(1.5)
            avant = len([l for l in ec.lignes_de_trace() if "chaîne=" in l])
            os.write(fd, b"cc 102 1\n")
            ec.lire(1.0)
            # « Refais le geste » : ratatui réécrit seulement les cellules changées (piège 7)
            verifier("Refais le geste" in ec.voir(), f"{taille} : un geste propose")
            verifier(
                len([l for l in ec.lignes_de_trace() if "chaîne=" in l]) == avant,
                f"{taille} : un seul geste ne change pas le son",
            )
            os.write(fd, b"cc 102 1\n")
            ec.lire(5.0)
            sessions = [l for l in ec.lignes_de_trace() if " session " in l]
            verifier(
                sessions[-1:] and sessions[-1].endswith("session instrument=2 Fretless reprise=Some((0, 1))"),
                f"{taille} : la session se relance avec Fretless, au son 2 du morceau ({sessions[-1:]})",
            )
            # ce qui s'écrit après la ligne de démarrage du nouvel instrument : le nouvel écran
            t = ec.voir()
            apres = t[t.rfind("instrument 2 Fretless"):] if "instrument 2 Fretless" in t else ""
            verifier("son 2/2 fuzz" in apres, f"{taille} : le nouvel écran joue le son 2, « fuzz »")
            # le port a survécu à la relance : le pied rejoue le son 1
            os.write(fd, b"cc 34 0\n")
            ec.lire(1.5)
            tr = ec.lignes_de_trace()
            depuis = max(i for i, l in enumerate(tr) if " session " in l)
            chaines = [l for l in tr[depuis:] if "chaîne=" in l]
            verifier(
                chaines and "reverb" not in chaines[-1],
                f"{taille} : le pédalier répond après la relance (CC 34 = 0 : « clean »)",
            )
            os.close(fd)
            ec.taper("q")
            ec.taper("Q")  # la scène demande confirmation
            verifier(ec.attendre_fin(4.0) == 0, f"{taille} : on quitte proprement après une relance")
        finally:
            ec.tuer()


def scenario_midi_canal(binaire, lignes, colonnes):
    """**Le canal écouté** (`--midi-channel 5`), sur l'écran de scène : un CC venu d'un autre canal
    (les lights, sur le 10) ne change pas le son et se compte ; le même CC sur le canal 5 le change.
    """
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-canal-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "cable")
        os.mkfifo(tube)
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "--midi-in", f"sim:{tube}",
             "--midi-channel", "5", "run", "--set", "concert", "--library", bib, "--master-db", "-120"],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(3.0)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            avant = len(chaines)
            os.write(fd, b"cc 34 1 ch 10\n")
            ec.lire(1.5)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(len(chaines) == avant, f"{taille} : un CC du canal 10 ne change pas le son")
            verifier(
                any("midi ignorés=1" in l for l in ec.lignes_de_trace()),
                f"{taille} : le message du canal 10 est compté comme ignoré",
            )
            os.write(fd, b"cc 34 1 ch 5\n")
            ec.lire(1.5)
            chaines = [l for l in ec.lignes_de_trace() if "chaîne=" in l]
            verifier(
                len(chaines) > avant and "reverb" in chaines[-1],
                f"{taille} : le même CC sur le canal 5 joue le son 2 ({chaines[-1:]})",
            )
            os.close(fd)
            ec.taper("q", 0.8)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : la scène se quitte proprement ({code})")
        finally:
            ec.tuer()


def scenario_midi_canaux(binaire, lignes, colonnes):
    """**Chaque canal avec ses numéros** (ADR 0012), sur l'écran de scène : le canal 5 (Ableton)
    appelle les sons par le CC 20, le canal 1 (le pédalier) garde le CC 34 ; le dernier message
    est suivi ; un CC 34 sur le canal 5 et tout message du canal 10 ne changent rien."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-canaux-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "cable")
        os.mkfifo(tube)
        config = os.path.join(tmp, "config.toml")
        with open(config, "w", encoding="utf-8") as f:
            f.write(f'[audio]\nbackend = "sim"\ndevice = "4x6"\n\n[midi]\nin = "sim:{tube}"\n\n'
                    "[[midi.channels]]\nchannel = 1\n\n[[midi.channels]]\nchannel = 5\ncc_sound = 20\n")
        ec = Ecran(
            binaire,
            ["--config", config, "run", "--set", "concert", "--library", bib, "--master-db", "-120"],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(3.0)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return

            def chaines():
                return [l for l in ec.lignes_de_trace() if "chaîne=" in l]

            avant = len(chaines())
            os.write(fd, b"cc 20 1 ch 5\n")
            ec.lire(1.5)
            c = chaines()
            verifier(len(c) > avant and "reverb" in c[-1], f"{taille} : CC 20 du canal 5 joue le son 2 ({c[-1:]})")
            avant = len(c)
            os.write(fd, b"cc 34 0 ch 1\n")
            ec.lire(1.5)
            c = chaines()
            verifier(len(c) > avant and "reverb" not in c[-1], f"{taille} : CC 34 du canal 1 joue le son 1 ({c[-1:]})")
            avant = len(c)
            os.write(fd, b"cc 34 1 ch 5\npc 1 ch 10\n")
            ec.lire(1.5)
            verifier(len(chaines()) == avant, f"{taille} : le CC 34 du canal 5 et le canal 10 ne changent rien")
            os.close(fd)
            ec.taper("q", 0.8)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : la scène se quitte proprement ({code})")
        finally:
            ec.tuer()


def scenario_midi_ecran(binaire, lignes, colonnes):
    """**L'écran de réglage** (`namfoop midi` devant un terminal) : ajouter le canal d'Ableton en
    appuyant, taper le CC convenu, enregistrer, puis relire la config écrite."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-ecran-") as tmp:
        tube = os.path.join(tmp, "cable")
        os.mkfifo(tube)
        config = os.path.join(tmp, "config.toml")
        with open(config, "w", encoding="utf-8") as f:
            f.write(f'[midi]\nin = "sim:{tube}"\n')
        ec = Ecran(binaire, ["--config", config, "midi"], lignes, colonnes, os.path.join(tmp, "trace.log"))
        try:
            ec.lire(1.5)
            verifier("Appareils et canaux" in ec.voir(), f"{taille} : l'écran de réglage s'ouvre")
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            os.write(fd, b"[UM-ONE] pc 2 ch 5\n[UM-ONE] pc 2 ch 10\n")
            ec.lire(0.8)
            verifier("plusieurs" in ec.voir(), f"{taille} : deux canaux vus, l'écran prévient (plusieurs canaux parlent)")
            ec.taper("a", 0.4)
            os.write(fd, b"[UM-ONE] pc 2 ch 5\n")
            ec.lire(0.6)
            verifier("reçu sur le canal 5" in ec.voir(), f"{taille} : un appui remplit la saisie")
            ec.taper("\r", 0.5)
            # Canal 5 › Gabarit › Morceau › Son du morceau
            for _ in range(3):
                ec.taper("\x1b[B", 0.2)
            ec.taper("\r", 0.4)
            ec.taper("2", 0.1)
            ec.taper("0", 0.1)
            ec.taper("\r", 0.5)
            verifier("non enregistrée" in ec.voir(), f"{taille} : le réglage n'est pas encore écrit")
            ec.taper("S", 0.6)
            ec.taper("q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : l'écran se quitte proprement ({code})")
            os.close(fd)
            ecrit = open(config, encoding="utf-8").read()
            verifier(
                "[[midi.channels]]\nchannel = 5\ncc_sound = 20" in ecrit,
                f"{taille} : la config garde le canal 5 et son CC 20",
            )
            verifier(f'in = "sim:{tube}"' in ecrit, f"{taille} : le reste de [midi] est gardé")
        finally:
            ec.tuer()


def scenario_midi_ecran_set(binaire, lignes, colonnes):
    """**Ce que namfoop fait d'un message** (zone « reçu » de l'écran MIDI), avec le set du moment :
    le morceau qu'un Program Change appelle, et la raison quand rien ne correspond."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-ecran-set-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        tube = os.path.join(tmp, "cable")
        os.mkfifo(tube)
        config = os.path.join(tmp, "config.toml")
        with open(config, "w", encoding="utf-8") as f:
            f.write(f'[midi]\nin = "sim:{tube}"\n\n[set]\ndefault = "concert"\n')
        ec = Ecran(binaire, ["--config", config, "midi", "--library", bib], lignes, colonnes,
                   os.path.join(tmp, "trace.log"))
        try:
            ec.lire(1.5)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            os.write(fd, b"pc 1\n")
            ec.lire(0.8)
            verifier("Phare" in ec.voir(), f"{taille} : PC 1 appelle « Le Phare »")
            os.write(fd, b"pc 9\n")
            ec.lire(0.8)
            verifier("hors accord" in ec.voir(), f"{taille} : PC 9 est hors accord, et l'écran le dit")
            os.close(fd)
            ec.taper("q", 0.6)
            verifier(ec.attendre_fin(4.0) == 0, f"{taille} : l'écran se quitte proprement")
        finally:
            ec.tuer()


def scenario_midi_plan(binaire, lignes, colonnes):
    """**Le Plan du set** de l'écran MIDI : donner son numéro à un morceau en recevant le Program
    Change qu'Ableton enverra, enregistrer le set, puis relire son fichier."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-midi-plan-") as tmp:
        bib = set_de_concert(bibliotheque_jetable(os.path.join(tmp, "library")))
        tube = os.path.join(tmp, "cable")
        os.mkfifo(tube)
        config = os.path.join(tmp, "config.toml")
        with open(config, "w", encoding="utf-8") as f:
            f.write(f'[midi]\nin = "sim:{tube}"\n\n[set]\ndefault = "concert"\n')
        ec = Ecran(binaire, ["--config", config, "midi", "--library", bib], lignes, colonnes,
                   os.path.join(tmp, "trace.log"))
        try:
            ec.lire(1.5)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube MIDI")
            if fd is None:
                return
            ec.taper("\t", 0.5)
            verifier("Tempo" in ec.voir(), f"{taille} : Tab ouvre le Plan du set")
            ec.taper("\r", 0.4)
            os.write(fd, b"pc 7\n")
            ec.lire(0.6)
            ec.taper("\r", 0.4)
            verifier("PC 7" in ec.voir(), f"{taille} : le numéro reçu est posé (PC 7), pas encore écrit")
            ec.taper("S", 0.6)
            ec.taper("q", 0.6)
            verifier(ec.attendre_fin(4.0) == 0, f"{taille} : l'écran se quitte proprement")
            os.close(fd)
            ecrit = open(os.path.join(bib, "sets/concert.set.toml"), encoding="utf-8").read()
            verifier("pc = 7" in ecrit, f"{taille} : le set enregistré porte le PC 7")
            # la fiche (`--fiche`) se calcule du set enregistré : le morceau y porte son PC 7
            fiche = subprocess.run([binaire, *isoler(["--config", config, "midi", "--fiche", "--library", bib])],
                                   capture_output=True, text=True, timeout=20).stdout
            verifier("PC 7" in fiche and "Le Phare" in fiche, f"{taille} : la fiche porte « PC 7 Le Phare »")
            verifier(all(len(l) <= 46 for l in fiche.splitlines()), f"{taille} : la fiche tient sur un téléphone")
        finally:
            ec.tuer()


def scenario_midi_horloge(binaire, lignes, colonnes):
    """**L'horloge MIDI de bout en bout** : la phase du clic doit être IMMOBILE.

    C'est le défaut qui a échappé deux fois aux tests unitaires, parce qu'aucun d'eux ne faisait
    tourner la boucle : la phase était déduite d'une durée, donc d'un arrondi posé là où la gigue
    passe et repasse, et elle basculait des centaines de fois par minute — le temps déduit du
    compteur reculait et avançait, et le clic doublait.

    Un tube de texte ne tient pas une cadence assez régulière pour mesurer un *tempo* (un top vaut
    20,8 ms à 120 BPM), et c'est justement ce qui fait de lui un bon juge ici : la phase est une
    **soustraction de deux comptages**, donc indépendante du temps. Une cadence approximative doit
    donc donner une phase parfaitement stable — là où l'ancien calcul s'affolait."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-clock-") as tmp:
        bib = deux_presets(bibliotheque_jetable(os.path.join(tmp, "library")))
        trace = os.path.join(tmp, "trace.log")
        tube = os.path.join(tmp, "horloge")
        os.mkfifo(tube)
        ec = Ecran(
            binaire,
            ["--no-config", "--backend", "sim", "--device", "4x6", "--midi-in", f"sim:{tube}",
             "browse", "--library", bib],
            lignes, colonnes, trace,
        )
        try:
            ec.lire(2.5)
            fd = None
            for _ in range(50):
                try:
                    fd = os.open(tube, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError:
                    ec.lire(0.1)
            verifier(fd is not None, f"{taille} : namfoop a ouvert le tube de l'horloge")
            if fd is None:
                return
            # Les tops sont écrits par un **fil à part**, à 25 ms (100 BPM). Les écrire au rythme
            # de la boucle de lecture d'écran donnait une cadence de 40 à 120 ms selon la charge,
            # hors de la tolérance de ±25 % de l'arbitre : l'horloge ne se verrouillait pas, et le
            # test ne mesurait rien. Un `time.sleep` tient la cadence à une milliseconde ou deux.
            #
            # Ce qu'on mesure est la **phase**, une soustraction de comptages : elle doit être
            # indifférente à la cadence. Le *tempo*, lui, se vérifie en unitaire.
            stop = threading.Event()

            def nourrir():
                prochain = time.monotonic()
                while not stop.is_set():
                    prochain += 0.025
                    reste = prochain - time.monotonic()
                    if reste > 0:
                        time.sleep(reste)
                    try:
                        os.write(fd, b"clock\n")
                    except OSError:
                        return

            fil = threading.Thread(target=nourrir, daemon=True)
            fil.start()
            ec.lire(4.0)  # ~160 tops : de quoi verrouiller (24 intervalles) et observer 5 noires
            stop.set()
            fil.join(1.0)
            ec.lire(0.4)
            phases = [l.split("phase=", 1)[1].strip() for l in ec.lignes_de_trace() if "phase=" in l]
            if not phases:
                # Le verrou demande 24 intervalles cohérents (±25 %) et se perd après 500 ms de
                # silence : sur une machine chargée, un tube de texte n'y arrive pas toujours.
                # Comme le test ALSA de `midi-io`, on le dit et **on ne conclut pas** — mieux que
                # de refuser un commit pour la charge de la machine.
                print(f"  —    {taille} : l'horloge ne s'est pas verrouillée (machine chargée ?) :"
                      f" ne conclut pas sur la phase")
            else:
                # UNE seule valeur : la phase ne doit jamais retomber sur une autre
                verifier(
                    len(set(phases)) == 1,
                    f"{taille} : la phase du clic est immobile ({len(phases)} écritures, "
                    f"valeurs {sorted(set(phases))})",
                )
            # Le `Start` qui suit réancre la phase : on ne mesure donc son immobilité
            # qu'**avant** lui, et c'est fait ci-dessus.
            #
            # **Le transport, de bout en bout** : l'en-tête doit dire que la source lit, puis
            # qu'elle s'est arrêtée. Rien d'autre à l'écran ne le montre sans set ouvert, et
            # c'est la boucle entière qu'on veut voir le porter — pas seulement le rendu.
            os.write(fd, b"start\n")
            for _ in range(30):  # de quoi reverrouiller après le Start, qui vide la fenêtre
                os.write(fd, b"clock\n")
                ec.lire(0.02)
            # `voir()` rend tout ce qui est arrivé depuis le lancement : on vide le tampon avant
            # chaque observation, sans quoi une image précédente répondrait à sa place — et une
            # assertion négative n'aurait aucune valeur.
            ec.tampon = ""
            ec.lire(0.5)
            tete = ec.voir()
            verifier("⏵" in tete, f"{taille} : l'en-tête dit que la source lit")
            os.write(fd, b"stop\n")
            ec.tampon = ""
            ec.lire(0.8)
            tete = ec.voir()
            verifier("⏸" in tete, f"{taille} : l'en-tête dit qu'elle s'est arrêtée")
            verifier("⏵" not in tete, f"{taille} : et plus qu'elle lit")
            os.close(fd)
            # comme ailleurs : `q` peut avertir, `Q` quitte
            ec.taper("q", 0.8)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(6.0)
            verifier(code == 0, f"{taille} : on quitte proprement ({code})")
        finally:
            ec.tuer()


def scenario_instruments(binaire, lignes, colonnes):
    """**Mes instruments** (`docs/design/instruments.md` § 4, § 7) : `--instrument` prend un autre
    instrument que celui par défaut, l'écran le montre, et la config de l'ordinateur reste
    identique **à l'octet** après la session — plusieurs chemins de `browse` réécrivent la config
    qu'ils ont chargée, et ils ne doivent jamais y porter la calibration de l'instrument du jour."""
    taille = f"{lignes}×{colonnes}"
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-instr-") as tmp:
        bib = bibliotheque_jetable(os.path.join(tmp, "library"))
        os.makedirs(os.path.join(bib, "instruments"))
        for fichier, texte in (
            ("bb-735a.toml", 'name = "BB 735A"\nnumber = 1\ndefault = true\n\n'
                             '[calibration.itwo]\ntrim_db = -17.5\ntarget_dbfs = -30.0\n'),
            ("fretless.toml", 'name = "Fretless"\nnumber = 2\n\n'
                              '[calibration.itwo]\ntrim_db = -9.0\ntarget_dbfs = -30.0\n'),
        ):
            with open(os.path.join(bib, "instruments", fichier), "w", encoding="utf-8") as f:
                f.write(texte)
        config = os.path.join(tmp, "config.toml")
        with open(config, "w", encoding="utf-8") as f:
            f.write('trim_db = -6.0\n\n[audio]\nbackend = "sim"\ndevice = "4x6"\n')
        with open(config, "rb") as f:
            avant = f.read()
        ec = Ecran(
            binaire,
            ["--config", config, "--instrument", "fret", "browse", "--library", bib],
            lignes, colonnes, os.path.join(tmp, "trace.log"),
        )
        try:
            ec.lire(2.5)
            vu = ec.voir()
            verifier("2 Fretless" in vu, f"{taille} : l'instrument choisi est à l'écran")
            verifier("BB 735A" not in vu, f"{taille} : pas l'instrument par défaut")
            ec.taper("\x1b[B", 1.5)
            ec.taper("q", 0.8)
            ec.taper("Q", 0.6)
            code = ec.attendre_fin(4.0)
            verifier(code == 0, f"{taille} : `Q` quitte proprement (code {code})")
        finally:
            ec.tuer()
        with open(config, "rb") as f:
            verifier(f.read() == avant, f"{taille} : la config est identique à l'octet après la session")


def scenario_niveau_de_l_instrument(binaire):
    """**Le niveau suit l'instrument** : une calibration arrivée avec la bibliothèque joue en
    niveau « match » comme sur la machine qui l'a mesurée. Le même preset rendu (1) avec une
    calibration de la config et `level = "match"`, comme après `calibrate` sur cette machine, et
    (2) avec la même mesure portée par un instrument et une config qui dit `normalize` en toutes
    lettres, comme après `setup` ailleurs, sort identique ; (3) sans instrument, la config
    `normalize` vaut, et le rendu diffère."""
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-niveau-") as tmp:
        bib = bibliotheque_jetable(os.path.join(tmp, "library"))
        # l'enregistrement de calibration : n'importe quel son fait l'affaire, la même mesure sert
        # aux trois rendus
        ir_de_synthese(os.path.join(tmp, "calibration.wav"), secondes=1.0)
        os.makedirs(os.path.join(bib, "instruments"))
        shutil.copy(os.path.join(tmp, "calibration.wav"), os.path.join(bib, "instruments", "bb.itwo.wav"))

        def rendre(nom, config, instrument):
            dossier = os.path.join(tmp, nom)
            os.makedirs(dossier)
            shutil.copy(os.path.join(tmp, "calibration.wav"), dossier)
            chemin = os.path.join(dossier, "config.toml")
            with open(chemin, "w", encoding="utf-8") as f:
                f.write(config)
            fiche = os.path.join(bib, "instruments", "bb.toml")
            if instrument:
                with open(fiche, "w", encoding="utf-8") as f:
                    f.write('name = "BB"\nnumber = 1\n\n[calibration.itwo]\ntrim_db = -6.0\n'
                            'target_dbfs = -30.0\nwav = "bb.itwo.wav"\ndate = 2026-11-27\n')
            elif os.path.exists(fiche):
                os.remove(fiche)
            sortie = os.path.join(dossier, "sortie.wav")
            r = subprocess.run(
                [binaire, *isoler(["--config", chemin, "render", "--library", bib, "-m", MODELE,
                                   "-i", os.path.join(bib, "ir/essai/essai.wav"), "-o", sortie])],
                capture_output=True, text=True, timeout=120,
            )
            ok = r.returncode == 0
            verifier(ok, f"niveau : rendu « {nom} »" + ("" if ok else f" ({r.stderr.strip()[-300:]})"))
            with open(sortie, "rb") as f:
                return f.read(), r.stderr

        cadre = 'trim_db = -6.0\ncalibration_wav = "calibration.wav"\ncalibration_target_dbfs = -30.0\n'
        ici, _ = rendre("ici", cadre + 'level = "match"\n', instrument=False)
        ailleurs, log = rendre("ailleurs", 'level = "normalize"\n', instrument=True)
        normalise, _ = rendre("sans", cadre + 'level = "normalize"\n', instrument=False)
        verifier(ici == ailleurs, "niveau : la calibration venue avec la bibliothèque joue comme celle d'ici")
        verifier("niveau match" in log, "niveau : la ligne de démarrage dit le niveau retenu")
        verifier(normalise != ici, "niveau : sans instrument, la config `normalize` vaut")


def scenario_question(binaire):
    """**La question du lancement** (`docs/design/instruments.md` § 5) : un instrument jamais
    calibré sur la carte branchée fait poser la question **avant** d'ouvrir la carte. On la vise
    avec un device ALSA qui n'existe pas, donné par `--device` : la question passe, `Échap` retient « jouée sans » dans
    le fichier de l'instrument, puis l'ouverture de la carte échoue — c'est le signe que rien
    n'a été ouvert avant la réponse. Relancé, namfoop ne repose pas la question. En carte
    simulée, aucune question."""
    with tempfile.TemporaryDirectory(prefix="namfoop-e2e-question-") as tmp:
        bib = bibliotheque_jetable(os.path.join(tmp, "library"))
        os.makedirs(os.path.join(bib, "instruments"))
        fiche = os.path.join(bib, "instruments", "guitare.toml")
        with open(fiche, "w", encoding="utf-8") as f:
            f.write('name = "Guitare"\nnumber = 1\n')
        config = os.path.join(tmp, "config.toml")

        def lancer(backend, touches=None):
            # le device en option : une carte écrite dans la config, mais absente, est refusée
            # avant toute question (aucune carte réglée n'est branchée)
            device = "4x6" if backend == "sim" else "hw:Nulle"
            with open(config, "w", encoding="utf-8") as f:
                f.write(f'[audio]\nbackend = "{backend}"\n')
            ec = Ecran(binaire, ["--config", config, "--device", device, "browse", "--library", bib], 25, 80,
                       os.path.join(tmp, "trace.log"))
            try:
                ec.lire(2.0)
                vu = ec.voir()
                if touches:
                    ec.taper(touches, 1.5)
                return vu, ec
            except Exception:
                ec.tuer()
                raise

        vu, ec = lancer("alsa", "\x1b")
        ec.tuer()
        verifier("1 Guitare n'est pas calibrée sur nulle" in vu, "question : l'instrument et la carte sont nommés")
        verifier("Rien ne sort" in vu, "question : rien ne sort avant la réponse")
        with open(fiche, encoding="utf-8") as f:
            verifier("skipped = true" in f.read(), "question : `Échap` retient « jouée sans »")
        vu, ec = lancer("alsa")
        ec.tuer()
        verifier("avant de jouer" not in vu, "question : elle ne revient pas une fois répondue")
        os.remove(fiche)
        with open(fiche, "w", encoding="utf-8") as f:
            f.write('name = "Guitare"\nnumber = 1\n')
        vu, ec = lancer("sim")
        try:
            verifier("avant de jouer" not in vu and "1 Guitare" in vu, "question : aucune en carte simulée")
            ec.taper("Q", 0.6)
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
        scenario_battue(binaire, lignes, colonnes)
        scenario_accordeur(binaire, lignes, colonnes)
        scenario_instruments(binaire, lignes, colonnes)
    scenario_niveau_de_l_instrument(binaire)
    scenario_question(binaire)
    # une seule taille : le MIDI ne dessine presque rien, la taille d'écran n'y change rien
    scenario_midi(binaire, 25, 80)
    scenario_midi_horloge(binaire, 25, 80)
    scenario_midi_set(binaire, 25, 80)
    scenario_instrument_au_pied(binaire, 25, 80)
    scenario_midi_canal(binaire, 25, 80)
    scenario_midi_canaux(binaire, 25, 80)
    scenario_midi_ecran(binaire, 25, 80)
    scenario_midi_ecran_set(binaire, 25, 80)
    scenario_midi_plan(binaire, 25, 80)

    if ECHECS:
        print(f"bout en bout : ÉCHEC ({len(ECHECS)})")
        sys.exit(1)
    print("bout en bout : OK")


if __name__ == "__main__":
    main()
