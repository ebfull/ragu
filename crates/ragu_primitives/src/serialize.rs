//! Traits for serializing gadgets into a sequence of [`Element`]s.

use ragu_core::{Result, drivers::Driver, gadgets::Gadget};

use crate::Element;

/// Represents a gadget that can be serialized into a sequence of [`Element`]s
/// that are written to a [`Buffer`].
pub trait GadgetSerialize<'dr, D: Driver<'dr>>: Gadget<'dr, D> {
    /// Serialize this gadget into wires that are written the provided buffer,
    /// using the driver to synthesize the elements if needed.
    fn serialize<B: Buffer<'dr, D>>(&self, dr: &mut D, buf: &mut B) -> Result<()>;
}

/// Represents a destination for values with some context `D`, such as a
/// [`Driver`].
pub trait Buffer<'dr, D: Driver<'dr>> {
    /// Push an `Element` into this buffer using the provided driver `D`.
    fn write(&mut self, dr: &mut D, value: &Element<'dr, D>) -> Result<()>;
}

/// Automatically derives the [`GadgetSerialize`] trait for gadgets that merely
/// contain other gadgets.
///
/// This only works for structs with named fields. Similar to the
/// [`Gadget`](derive@Gadget) derive macro, the driver type can be annotated
/// with `#[ragu(driver)]`. Fields with `#[ragu(skip)]` annotations are ignored.
///
/// ## Example
///
/// ```rust
/// # use arithmetic::CurveAffine;
/// # use ragu_core::{drivers::{Driver, Witness}, gadgets::Gadget};
/// # use ragu_primitives::{Element, serialize::GadgetSerialize};
/// # use core::marker::PhantomData;
/// #[derive(Gadget, GadgetSerialize)]
/// pub struct Point<'dr, D: Driver<'dr>, C: CurveAffine> {
///     #[ragu(gadget)]
///     x: Element<'dr, D>,
///     #[ragu(gadget)]
///     y: Element<'dr, D>,
///     #[ragu(phantom)]
///     #[ragu(skip)]
///     _marker: PhantomData<C>,
/// }
/// ```
pub use ragu_macros::GadgetSerialize;
