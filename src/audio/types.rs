#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug)]
pub struct AudioBlock {
    pub format: AudioFormat,
    pub samples: Vec<f32>,
    pub frames: usize,
}
