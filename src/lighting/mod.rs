//! Engine-independent light accumulation and light colour lookup.

mod colors;
mod lut;
mod math;

pub use colors::*;
pub use lut::*;
pub use math::*;

#[cfg(test)]
mod tests;
