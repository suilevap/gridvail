use std::ops::Add;

/// A path cost: totally ordered and summed step by step. Step costs must
/// never be negative, which unsigned types and [`Lex`] of them guarantee.
pub trait Cost: Copy + Ord + Add<Output = Self> {
    const ZERO: Self;

    /// `n` steps of this cost each, saturating; used for lower bounds.
    fn times(self, n: u32) -> Self;

    /// The cost of `n` plain steps, saturating; used for route penalties.
    fn steps(n: u32) -> Self;
}

macro_rules! unsigned_cost {
    ($($t:ty),*) => {$(
        impl Cost for $t {
            const ZERO: Self = 0;

            fn times(self, n: u32) -> Self {
                self.saturating_mul(<$t>::try_from(n).unwrap_or(<$t>::MAX))
            }

            fn steps(n: u32) -> Self {
                <$t>::try_from(n).unwrap_or(<$t>::MAX)
            }
        }
    )*};
}

unsigned_cost!(u16, u32, u64);

/// Two costs compared lexicographically: `0` decides, `1` breaks ties.
/// `Lex(keys_used, steps)` prefers any path with fewer keys, then the
/// shortest. Plain steps (penalties) count toward `1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lex<A, B>(pub A, pub B);

impl<A: Add<Output = A>, B: Add<Output = B>> Add for Lex<A, B> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Lex(self.0 + other.0, self.1 + other.1)
    }
}

impl<A: Cost, B: Cost> Cost for Lex<A, B> {
    const ZERO: Self = Lex(A::ZERO, B::ZERO);

    fn times(self, n: u32) -> Self {
        Lex(self.0.times(n), self.1.times(n))
    }

    fn steps(n: u32) -> Self {
        Lex(A::ZERO, B::steps(n))
    }
}
