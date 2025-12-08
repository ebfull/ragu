use super::{MaybeCast, MaybeKind};

impl<const N: usize, U: Send, K: MaybeKind> MaybeCast<[U; N], K> for [U; N] {
    type Output = [K::Rebind<U>; N];

    fn empty() -> Self::Output {
        core::array::from_fn(|_| K::empty())
    }
    fn cast(self) -> Self::Output {
        // TODO(ebfull): This can be done more efficiently with unsafe{} code,
        // since the two structures have identical layouts.
        let mut iter = self.into_iter();
        core::array::from_fn(|_| K::maybe_just(|| iter.next().expect("array lengths are the same")))
    }
}

macro_rules! impl_maybe_cast_tuple {
    ($($idx:tt: $T:ident),+) => {
        impl<$($T: Send,)+ K: MaybeKind> MaybeCast<($($T,)+), K> for ($($T,)+) {
            type Output = ($(K::Rebind<$T>,)+);

            fn empty() -> Self::Output {
                ($(impl_maybe_cast_tuple!(@empty $idx),)+)
            }
            fn cast(self) -> Self::Output {
                ($(K::maybe_just(|| self.$idx),)+)
            }
        }
    };
    (@empty $idx:tt) => { K::empty() };
}

impl_maybe_cast_tuple!(0: T0, 1: T1);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26, 27: T27);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26, 27: T27, 28: T28);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26, 27: T27, 28: T28, 29: T29);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26, 27: T27, 28: T28, 29: T29, 30: T30);
impl_maybe_cast_tuple!(0: T0, 1: T1, 2: T2, 3: T3, 4: T4, 5: T5, 6: T6, 7: T7, 8: T8, 9: T9, 10: T10, 11: T11, 12: T12, 13: T13, 14: T14, 15: T15, 16: T16, 17: T17, 18: T18, 19: T19, 20: T20, 21: T21, 22: T22, 23: T23, 24: T24, 25: T25, 26: T26, 27: T27, 28: T28, 29: T29, 30: T30, 31: T31);

#[cfg(test)]
use super::{Always, Empty, Maybe};

#[test]
fn test_2tuple() {
    let (a, b) = Always::maybe_just(|| (1usize, 2usize)).cast();
    assert_eq!(a.take(), 1);
    assert_eq!(b.take(), 2);
    let (Empty, Empty) = <Empty as Maybe<(usize, usize)>>::cast(Empty);
}

#[test]
fn test_3tuple() {
    let (a, b, c) = Always::maybe_just(|| (1usize, 2usize, 3usize)).cast();
    assert_eq!(a.take(), 1);
    assert_eq!(b.take(), 2);
    assert_eq!(c.take(), 3);
    let (Empty, Empty, Empty) = <Empty as Maybe<(usize, usize, usize)>>::cast(Empty);
}

#[test]
fn test_4tuple_full() {
    let (a, b, c, d) =
        Always::maybe_just(|| (1usize, 2usize, 3usize, 4usize)).cast::<(_, _, _, _)>();
    assert_eq!(a.take(), 1);
    assert_eq!(b.take(), 2);
    assert_eq!(c.take(), 3);
    assert_eq!(d.take(), 4);
    let (Empty, Empty, Empty, Empty) =
        <Empty as Maybe<(usize, usize, usize, usize)>>::cast::<(_, _, _, _)>(Empty);
}

#[test]
fn test_arr() {
    let [a, b, c] = Always::maybe_just(|| [1usize, 2usize, 3usize]).cast();
    assert_eq!(a.take(), 1);
    assert_eq!(b.take(), 2);
    assert_eq!(c.take(), 3);
    let [Empty, Empty, Empty] = <Empty as Maybe<[usize; 3]>>::cast(Empty);
}

#[test]
fn test_5tuple() {
    let (a, b, c, d, e) = Always::maybe_just(|| (1usize, 2usize, 3usize, 4usize, 5usize)).cast();
    assert_eq!(a.take(), 1);
    assert_eq!(b.take(), 2);
    assert_eq!(c.take(), 3);
    assert_eq!(d.take(), 4);
    assert_eq!(e.take(), 5);
    let (Empty, Empty, Empty, Empty, Empty) =
        <Empty as Maybe<(usize, usize, usize, usize, usize)>>::cast(Empty);
}
