use ff::Field;
use ragu_circuits::Circuit;
use ragu_core::{
    Result,
    drivers::{Driver, DriverValue},
    gadgets::{GadgetKind, Kind},
};
use ragu_primitives::Element;

pub struct Dummy;

impl<F: Field> Circuit<F> for Dummy {
    type Instance<'source> = ();
    type Witness<'source> = ();
    type Output = Kind![F; (Element<'_, _>, Element<'_, _>)];
    type Aux<'source> = ();

    fn instance<'dr, 'source: 'dr, D: Driver<'dr, F = F>>(
        &self,
        _: &mut D,
        _: DriverValue<D, Self::Instance<'source>>,
    ) -> Result<<Self::Output as GadgetKind<F>>::Rebind<'dr, D>> {
        Ok((Element::one(), Element::one()))
    }

    fn witness<'dr, 'source: 'dr, D: Driver<'dr, F = F>>(
        &self,
        _: &mut D,
        _: DriverValue<D, Self::Witness<'source>>,
    ) -> Result<(
        <Self::Output as GadgetKind<F>>::Rebind<'dr, D>,
        DriverValue<D, Self::Aux<'source>>,
    )> {
        Ok(((Element::one(), Element::one()), D::just(|| ())))
    }
}
