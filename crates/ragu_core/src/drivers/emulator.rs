use ff::Field;

use crate::{
    Result,
    drivers::{Coeff, Driver, DriverTypes},
    maybe::{Always, MaybeKind},
};

/// A driver used to execute circuit synthesis code and obtain the result of a
/// computation without enforcing constraints or collecting a witness. Useful
/// for obtaining the result of a computation that is later executed with
/// another driver.
pub struct Emulator<F: Field> {
    _marker: core::marker::PhantomData<F>,
}

impl<F: Field> Default for Emulator<F> {
    fn default() -> Self {
        Emulator::new()
    }
}

impl<F: Field> Emulator<F> {
    /// Creates a new `Emulator` driver.
    pub fn new() -> Self {
        Emulator {
            _marker: core::marker::PhantomData,
        }
    }

    /// Executes a closure with this driver, returning its output.
    pub fn just<R, W: Send>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        f(self)
    }

    /// Executes a closure with this driver, passing a witness value into the
    /// closure and returning its output.
    pub fn with<R, W: Send>(
        &mut self,
        witness: W,
        f: impl FnOnce(&mut Self, Always<W>) -> Result<R>,
    ) -> Result<R> {
        f(self, Always::maybe_just(|| witness))
    }
}

impl<F: Field> DriverTypes for Emulator<F> {
    type ImplField = F;
    type ImplWire = ();
    type MaybeKind = Always<()>;
    type LCadd = ();
    type LCenforce = ();
}

impl<F: Field> Driver<'_> for Emulator<F> {
    type F = F;
    type Wire = ();
    const ONE: Self::Wire = ();

    fn alloc(&mut self, _: impl Fn() -> Result<Coeff<Self::F>>) -> Result<()> {
        Ok(())
    }

    fn constant(&mut self, _: Coeff<Self::F>) -> Self::Wire {
        ()
    }

    fn mul(
        &mut self,
        _: impl Fn() -> Result<(Coeff<Self::F>, Coeff<Self::F>, Coeff<Self::F>)>,
    ) -> Result<(Self::Wire, Self::Wire, Self::Wire)> {
        Ok(((), (), ()))
    }

    fn add(&mut self, _: impl Fn(Self::LCadd) -> Self::LCadd) -> Self::Wire {
        ()
    }

    fn enforce_zero(&mut self, _: impl Fn(Self::LCenforce) -> Self::LCenforce) -> Result<()> {
        Ok(())
    }
}
