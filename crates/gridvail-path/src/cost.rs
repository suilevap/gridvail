use std::ops::Add;

/// A path cost: totally ordered and summed step by step. Step costs must
/// never be negative, which unsigned types and [`Lex`] of them guarantee.
pub trait PathCost: Copy + Ord + Add<Output = Self> {
    const ZERO: Self;

    /// `moves` steps of this cost each, saturating; used for lower bounds.
    fn times(self, moves: u32) -> Self;
}

macro_rules! unsigned_cost {
    ($($unsigned:ty),*) => {$(
        impl PathCost for $unsigned {
            const ZERO: Self = 0;

            fn times(self, moves: u32) -> Self {
                self.saturating_mul(<$unsigned>::try_from(moves).unwrap_or(<$unsigned>::MAX))
            }
        }
    )*};
}

unsigned_cost!(u16, u32, u64);

/// Two costs compared lexicographically: the primary decides, the secondary
/// breaks ties. `Lex(keys_used, steps)` prefers any path with fewer keys,
/// then the shortest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lex<Primary, Secondary>(pub Primary, pub Secondary);

impl<Primary: Add<Output = Primary>, Secondary: Add<Output = Secondary>> Add
    for Lex<Primary, Secondary>
{
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Lex(self.0 + other.0, self.1 + other.1)
    }
}

impl<Primary: PathCost, Secondary: PathCost> PathCost for Lex<Primary, Secondary> {
    const ZERO: Self = Lex(Primary::ZERO, Secondary::ZERO);

    fn times(self, moves: u32) -> Self {
        Lex(self.0.times(moves), self.1.times(moves))
    }
}
