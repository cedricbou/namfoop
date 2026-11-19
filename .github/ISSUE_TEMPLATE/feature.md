---
name: Fonctionnalité
about: Une évolution de namfoop
labels: enhancement
---

## Besoin
<!-- Le problème rencontré en jouant / en configurant, et pourquoi. -->

## Attendu
<!-- Le comportement visé, les commandes / options, ce qui est enregistré. -->

## Crates touchées
<!-- nam-sys, dsp, audio-io, model, engine, library, namfoop (voir docs/architecture.md).
     Plus de deux crates : découper l'issue, ou justifier ici pourquoi. -->

## Tests
- [ ] Sans matériel : tests automatiques, `--backend sim`, `namfoop render`
- [ ] Avec matériel (si l'audio est touché) : `scripts/test-backends.sh` (ALSA, avec la carte)
