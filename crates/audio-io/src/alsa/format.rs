//! Formats d'échantillons ALSA acceptés et conversions entrelacé (octets) <-> f32 désentrelacé.
//! Appelées dans la boucle audio : aucune allocation, longueurs vérifiées par l'appelant.

use alsa::pcm::Format;

/// Format d'échantillon négocié avec la carte, petit-boutiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    /// 32 bits signés (iTwo).
    S32,
    /// 24 bits signés sur 3 octets (cartes USB Behringer...).
    S24In3,
    /// 24 bits signés dans un conteneur de 32 bits (bits de poids faible).
    S24In4,
    /// 16 bits signés (HAT I2S...).
    S16,
}

/// Échelle S32 : 2^31. En sortie, 2^31 - 128 (plus grand f32 sous 2^31) pour ne pas déborder.
const S32_SCALE: f32 = 2147483648.0;
const S24_SCALE: f32 = 8388608.0; // 2^23
const S16_SCALE: f32 = 32768.0;

impl SampleFormat {
    /// Ordre d'essai : le premier accepté par la carte gagne.
    pub const PREFERENCE: [SampleFormat; 4] =
        [SampleFormat::S32, SampleFormat::S24In3, SampleFormat::S24In4, SampleFormat::S16];

    pub fn alsa(self) -> Format {
        match self {
            SampleFormat::S32 => Format::S32LE,
            SampleFormat::S24In3 => Format::S243LE,
            SampleFormat::S24In4 => Format::S24LE,
            SampleFormat::S16 => Format::S16LE,
        }
    }

    /// Nom ALSA (« S24_3LE »...).
    pub fn name(self) -> &'static str {
        match self {
            SampleFormat::S32 => "S32_LE",
            SampleFormat::S24In3 => "S24_3LE",
            SampleFormat::S24In4 => "S24_LE",
            SampleFormat::S16 => "S16_LE",
        }
    }

    /// Octets par échantillon.
    pub fn bytes(self) -> usize {
        match self {
            SampleFormat::S32 | SampleFormat::S24In4 => 4,
            SampleFormat::S24In3 => 3,
            SampleFormat::S16 => 2,
        }
    }

    fn read(self, b: &[u8]) -> f32 {
        match self {
            SampleFormat::S32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / S32_SCALE,
            // extension de signe : les 24 bits sont placés en haut d'un i32, puis décalés
            SampleFormat::S24In3 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / S24_SCALE,
            SampleFormat::S24In4 => {
                (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) << 8 >> 8) as f32 / S24_SCALE
            }
            SampleFormat::S16 => i16::from_le_bytes([b[0], b[1]]) as f32 / S16_SCALE,
        }
    }

    fn write(self, x: f32, b: &mut [u8]) {
        let x = x.clamp(-1.0, 1.0);
        match self {
            SampleFormat::S32 => b.copy_from_slice(&((x * (S32_SCALE - 128.0)) as i32).to_le_bytes()),
            SampleFormat::S24In3 => b.copy_from_slice(&((x * (S24_SCALE - 1.0)) as i32).to_le_bytes()[..3]),
            SampleFormat::S24In4 => b.copy_from_slice(&((x * (S24_SCALE - 1.0)) as i32).to_le_bytes()),
            SampleFormat::S16 => b.copy_from_slice(&((x * (S16_SCALE - 1.0)) as i16).to_le_bytes()),
        }
    }

    /// Octets entrelacés (`frames` × `ins.len()` échantillons) -> un buffer f32 par canal.
    pub fn decode(self, raw: &[u8], ins: &mut [Vec<f32>], frames: usize) {
        let (bps, ch) = (self.bytes(), ins.len());
        for (f, frame) in raw[..frames * ch * bps].chunks_exact(ch * bps).enumerate() {
            for (c, s) in frame.chunks_exact(bps).enumerate() {
                ins[c][f] = self.read(s);
            }
        }
    }

    /// Un buffer f32 par canal -> octets entrelacés, écrêtés à ±1.0.
    pub fn encode(self, outs: &[Vec<f32>], raw: &mut [u8], frames: usize) {
        let (bps, ch) = (self.bytes(), outs.len());
        for (f, frame) in raw[..frames * ch * bps].chunks_exact_mut(ch * bps).enumerate() {
            for (c, s) in frame.chunks_exact_mut(bps).enumerate() {
                self.write(outs[c][f], s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SampleFormat::*;

    fn roundtrip(fmt: SampleFormat, x: f32) -> f32 {
        let mut raw = [0u8; 4];
        fmt.write(x, &mut raw[..fmt.bytes()]);
        fmt.read(&raw[..fmt.bytes()])
    }

    #[test]
    fn roundtrip_every_format() {
        for fmt in SampleFormat::PREFERENCE {
            let lsb = match fmt {
                S32 => 1.0 / S32_SCALE * 256.0, // précision du f32
                S24In3 | S24In4 => 1.0 / S24_SCALE,
                S16 => 1.0 / S16_SCALE,
            };
            for x in [0.0, 0.5, -0.5, 0.123_456, -0.987_654, 1e-3] {
                let y = roundtrip(fmt, x);
                assert!((x - y).abs() <= 2.0 * lsb, "{} : {x} -> {y}", fmt.name());
            }
        }
    }

    #[test]
    fn full_scale_and_clipping() {
        for fmt in SampleFormat::PREFERENCE {
            for (x, want) in [(1.0, 1.0), (-1.0, -1.0), (3.0, 1.0), (-7.5, -1.0)] {
                let y = roundtrip(fmt, x);
                assert!((y - want).abs() < 2e-4, "{} : {x} -> {y}", fmt.name());
            }
        }
        // valeurs brutes extrêmes
        assert_eq!(S16.read(&i16::MIN.to_le_bytes()), -1.0);
        assert_eq!(S32.read(&i32::MIN.to_le_bytes()), -1.0);
        let mut b = [0u8; 2];
        S16.write(1.0, &mut b);
        assert_eq!(i16::from_le_bytes(b), i16::MAX);
        S16.write(-1.0, &mut b);
        assert_eq!(i16::from_le_bytes(b), -i16::MAX);
    }

    /// Le comportement S32 est exactement celui d'avant la négociation (iTwo).
    #[test]
    fn s32_matches_previous_conversion() {
        for v in [i32::MIN, -1, 0, 1, 123_456_789, i32::MAX] {
            assert_eq!(S32.read(&v.to_le_bytes()), v as f32 / S32_SCALE);
        }
        for x in [-1.0f32, -0.3, 0.0, 0.7, 1.0, 2.0] {
            let mut b = [0u8; 4];
            S32.write(x, &mut b);
            assert_eq!(i32::from_le_bytes(b), (x.clamp(-1.0, 1.0) * (S32_SCALE - 128.0)) as i32);
        }
    }

    #[test]
    fn s24_sign_extension() {
        // S24_3LE : 0x800000 = -2^23 -> -1.0 ; 0xFFFFFF = -1 LSB ; 0x7FFFFF = +max
        assert_eq!(S24In3.read(&[0x00, 0x00, 0x80]), -1.0);
        assert_eq!(S24In3.read(&[0xff, 0xff, 0xff]), -1.0 / S24_SCALE);
        assert_eq!(S24In3.read(&[0xff, 0xff, 0x7f]), (S24_SCALE - 1.0) / S24_SCALE);
        // S24_LE : l'octet de poids fort du conteneur est ignoré (0 ou extension de signe)
        assert_eq!(S24In4.read(&[0x00, 0x00, 0x80, 0x00]), -1.0);
        assert_eq!(S24In4.read(&[0x00, 0x00, 0x80, 0xff]), -1.0);
        assert_eq!(S24In4.read(&[0xff, 0xff, 0xff, 0x00]), -1.0 / S24_SCALE);
        assert_eq!(S24In4.read(&[0xff, 0xff, 0x7f, 0x00]), (S24_SCALE - 1.0) / S24_SCALE);
        // écriture d'une valeur négative sur 4 octets : conteneur à extension de signe
        let mut b = [0u8; 4];
        S24In4.write(-0.5, &mut b);
        assert_eq!(b[3], 0xff);
        assert!((S24In4.read(&b) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn interleaving_and_no_allocation() {
        use assert_no_alloc::assert_no_alloc;
        let frames = 48;
        for fmt in SampleFormat::PREFERENCE {
            let outs: Vec<Vec<f32>> =
                (0..3).map(|c| (0..frames).map(|f| ((f * 3 + c) as f32 / 200.0) - 0.5).collect()).collect();
            let mut raw = vec![0u8; frames * 3 * fmt.bytes() + 7]; // plus grand : seul le début compte
            let mut back = vec![vec![0f32; frames]; 3];
            assert_no_alloc(|| {
                fmt.encode(&outs, &mut raw, frames);
                fmt.decode(&raw, &mut back, frames);
            });
            for c in 0..3 {
                for f in 0..frames {
                    assert!((outs[c][f] - back[c][f]).abs() < 1e-4, "{} canal {c} frame {f}", fmt.name());
                }
            }
            // canal 1 de la frame 2 = échantillon n° 2*3+1 dans le flux entrelacé
            let k = (2 * 3 + 1) * fmt.bytes();
            assert!((fmt.read(&raw[k..k + fmt.bytes()]) - outs[1][2]).abs() < 1e-4);
        }
    }
}
