use crate::{db_to_gain, Stage};

/// Gain fixe (en dB à la construction).
pub struct Gain {
    gain: f32,
    name: String,
}

impl Gain {
    pub fn db(db: f32, name: impl Into<String>) -> Self {
        Self { gain: db_to_gain(db), name: name.into() }
    }
}

impl Stage for Gain {
    fn process(&mut self, buf: &mut [f32]) {
        if self.gain != 1.0 {
            for x in buf {
                *x *= self.gain;
            }
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
}
