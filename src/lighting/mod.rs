//! Engine-independent light accumulation and palette conversion.

mod math;
mod palette;

pub use math::*;
pub use palette::*;

#[cfg(test)]
mod tests;
