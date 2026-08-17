use ragu_arithmetic::{Cycle as _, ff::Field as _};
use ragu_core::{
    drivers::emulator::{Emulator, Wireless},
    maybe::{Always, Maybe as _},
};
use ragu_pasta::{Fp, Pasta};
use ragu_primitives::{
    Element,
    poseidon::{SaveError, Sponge as InnerSponge},
};

use crate::sponge::Sponge;

/// Distinct nonzero values, so any reordering or substitution is detectable.
fn inputs(n: u32) -> alloc::vec::Vec<Fp> {
    (0..n)
        .map(|i| Fp::from(u64::from(i) + 1) * Fp::from(7u64))
        .collect()
}

/// Absorb `values` then squeeze once.
fn absorb_then_squeeze(values: &[Fp]) -> Fp {
    let mut sponge = Sponge::new();
    for &v in values {
        sponge.absorb(v).unwrap();
    }
    sponge.squeeze().unwrap()
}

/// Drive the real in-circuit sponge directly through the wireless emulator,
/// bypassing the mock wrapper, as the source of truth.
fn reference_squeeze(values: &[Fp]) -> Fp {
    type Emu = Emulator<Wireless<Always<()>, Fp>>;
    let mut emu = Emu::execute();
    let mut sponge = InnerSponge::new(&mut emu, Pasta::circuit_poseidon(Pasta::baked()));
    for &v in values {
        let element = Element::constant(&mut emu, v);
        sponge.absorb(&mut emu, &element).unwrap();
    }
    *sponge.squeeze(&mut emu).unwrap().value().take()
}

#[test]
fn squeeze_is_deterministic() {
    let values = inputs(3);
    assert_eq!(absorb_then_squeeze(&values), absorb_then_squeeze(&values));
}

#[test]
fn distinct_inputs_give_distinct_squeezes() {
    assert_ne!(
        absorb_then_squeeze(&inputs(3)),
        absorb_then_squeeze(&inputs(4))
    );
}

#[test]
fn matches_the_real_sponge() {
    for n in 1..=6 {
        let values = inputs(n);
        assert_eq!(
            absorb_then_squeeze(&values),
            reference_squeeze(&values),
            "mock sponge diverged from the real sponge at {n} inputs",
        );
    }
}

#[test]
fn save_resume_round_trips() {
    // Saving after absorbing, then resuming and squeezing, must equal an
    // uninterrupted absorb -> squeeze of the same sequence.
    let values = inputs(3);

    let mut sponge = Sponge::new();
    for &v in &values {
        sponge.absorb(v).unwrap();
    }
    let state = sponge.save_state().unwrap();
    let resumed = Sponge::resume(state).squeeze().unwrap();

    assert_eq!(resumed, absorb_then_squeeze(&values));
}

#[test]
fn save_state_rejects_empty_sponge() {
    let err = Sponge::new().save_state().unwrap_err();
    assert_eq!(
        err.initialization_source::<SaveError>(),
        Some(&SaveError::NothingAbsorbed)
    );
}

#[test]
fn save_state_rejects_squeeze_mode() {
    let mut sponge = Sponge::new();
    sponge.absorb(Fp::ONE).unwrap();
    sponge.squeeze().unwrap();
    let err = sponge.save_state().unwrap_err();
    assert_eq!(
        err.initialization_source::<SaveError>(),
        Some(&SaveError::AlreadyInSqueezeMode)
    );
}
