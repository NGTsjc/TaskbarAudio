#[derive(Clone, Copy, Debug)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioEnergy {
    pub rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug)]
pub struct AudioBlock {
    pub format: AudioFormat,
    pub samples: Vec<f32>,
    pub frames: usize,
    pub energy: AudioEnergy,
    pub timestamp_qpc: i64,
}

