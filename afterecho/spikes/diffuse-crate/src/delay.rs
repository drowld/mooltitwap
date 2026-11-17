//! The two `DelayLine` calls Afterecho's `diffusion.rs` uses, copied from the
//! plug-in's `src/delay.rs` (`new`, `reset`, `write`, `back`), so the spike
//! runs the plug-in's own Diffuse file unchanged.

pub struct DelayLine {
    buffer: Vec<f32>,
    write_idx: usize,
    capacity: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let capacity = max_samples.next_power_of_two().max(2);
        Self { buffer: vec![0.0; capacity], write_idx: 0, capacity }
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        let mask = self.capacity - 1;
        self.buffer[self.write_idx & mask] = x;
        self.write_idx = (self.write_idx + 1) & mask;
    }

    #[inline]
    pub fn back(&self, samples: usize) -> f32 {
        self.buffer[(self.write_idx + self.capacity - samples) & (self.capacity - 1)]
    }
}
