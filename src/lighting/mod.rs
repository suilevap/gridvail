//! Engine-independent light accumulation, bounce and palette conversion.

mod bounce;
mod math;
mod palette;

pub use bounce::*;
pub use math::*;
pub use palette::*;

#[cfg(test)]
mod tests;
