use crate::clamped::*;
use crate::error::*;
use crate::{DEFAULT_LEVEL, STEPS_IN_REFERENCE_RANGE};

#[derive(Default)]
pub struct ScaleBuilder {
    kind: Option<ScaleKind>,
    max_value: Option<usize>,
    min_value: Option<usize>,
    ref_max: Option<usize>,
    ref_min: Option<usize>,
}

impl ScaleBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn kind(&mut self, v: ScaleKind) -> &mut Self {
        self.kind = Some(v);
        self
    }
    pub fn max_value(&mut self, v: usize) -> &mut Self {
        self.max_value = Some(v);
        self
    }
    pub fn min_value(&mut self, v: usize) -> &mut Self {
        self.min_value = Some(v);
        self
    }
    pub fn ref_max_value(&mut self, v: usize) -> &mut Self {
        self.ref_max = Some(v);
        self
    }
    pub fn ref_min_value(&mut self, v: usize) -> &mut Self {
        self.ref_min = Some(v);
        self
    }
    pub fn make(self) -> Result<BrightnessScale, Error> {
        let max_value = self.max_value.ok_or(Error::MaxBrightnessRequired)?;
        let min_value = self.min_value.unwrap_or(0);
        let ref_max = self.ref_max.map(|x| x as f32).unwrap_or(max_value as f32);
        let ref_min = self.ref_min.map(|x| x as f32).unwrap_or(min_value as f32);
        // Assume linear scale if not specified
        let kind = self.kind.unwrap_or(ScaleKind::Linear);
        let idx_factor = Self::idx_factor(&kind, ref_max, ref_min);
        Ok(BrightnessScale {
            kind,
            idx_factor,
            max_value,
            min_value,
            ref_max,
            ref_min,
            level: DEFAULT_LEVEL,
        })
    }
    fn idx_factor(kind: &ScaleKind, ref_max: f32, ref_min: f32) -> f32 {
        match kind {
            ScaleKind::Linear => Self::linear_factor(ref_max, ref_min),
            ScaleKind::Exp2(_) => Self::exp2_factor(ref_max, ref_min),
        }
    }
    fn linear_factor(ref_max: f32, ref_min: f32) -> f32 {
        (ref_max - ref_min) / STEPS_IN_REFERENCE_RANGE
    }
    fn exp2_factor(ref_max: f32, ref_min: f32) -> f32 {
        let ref_max_exp = f32::log2(ref_max);
        let ref_min_exp = f32::log2(ref_min);
        let stops = ref_max_exp - ref_min_exp;
        stops / STEPS_IN_REFERENCE_RANGE
    }
}

#[derive(Debug, PartialEq)]
pub enum ScaleKind {
    Linear,
    Exp2(f32),
}

#[derive(Debug, PartialEq)]
pub struct BrightnessScale {
    kind: ScaleKind,
    // scale factor for the desired brightness level. For linear type,
    // this is the step size. For exponential type, this is the multiplier
    // for the exponent.
    idx_factor: f32,
    max_value: usize,
    min_value: usize,
    ref_max: f32,
    ref_min: f32,
    // current brightness level. 0-9 is the reference range.
    level: i8,
}

impl BrightnessScale {
    pub fn value_for(&self, f: i8) -> ClampedValue<usize> {
        let f = f as f32 * self.idx_factor;
        let ref_max = self.ref_max as f32;
        let x = match self.kind {
            ScaleKind::Linear => ref_max - f,
            ScaleKind::Exp2(gamma) => ref_max / f32::powf(gamma, f),
        } as usize;
        ClampedValue::new(x, self.min_value, self.max_value)
    }
    pub fn get_brightness(&self) -> ClampedValue<usize> {
        self.value_for(self.level)
    }
    pub fn up(&mut self) -> ClampedValue<usize> {
        self.level -= 1;
        self.value_for(self.level)
    }
    pub fn down(&mut self) -> ClampedValue<usize> {
        self.level += 1;
        self.value_for(self.level)
    }
    pub fn set_level(&mut self, value: i8) -> ClampedValue<usize> {
        self.level = value;
        self.value_for(self.level)
    }
    pub fn set_to_default(&mut self) -> ClampedValue<usize> {
        self.set_level(DEFAULT_LEVEL)
    }
}
