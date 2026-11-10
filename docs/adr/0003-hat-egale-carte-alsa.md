# ADR 0003 · Un HAT I2S est une carte ALSA plus un profil

**Contexte.** Le matériel va évoluer : iTwo, puis UMC404HD, puis un HAT I2S sur le Pi. Linux
présente tous ces matériels comme des cartes ALSA ; ils diffèrent par les formats acceptés
(`S32_LE`, `S24_3LE`, `S16_LE`...), les réglages qui marchent et les contrôles mixer (gain du
préampli, sélection d'entrée).

**Décision.** Pas de nouveau backend par carte. Le backend ALSA négocie le format (#51) et les
particularités de chaque carte vivent dans un profil TOML (device, buffer, entrées, `[mixer]`,
latence mesurée), appliqué à l'ouverture (#58).

**Conséquences.** Ajouter une carte = écrire un profil et le valider sur le matériel, sans code.
Le code de conversion de format est testé sans carte. Pas de crate par carte son.
