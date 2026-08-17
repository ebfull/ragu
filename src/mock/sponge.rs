//! Out-of-circuit Poseidon sponge — wraps `ragu_primitives::poseidon::Sponge`.
//!
//! No sponge or permutation logic is copied here. Real ragu's sponge API is
//! inherently in-circuit: every method threads a driver and operates on
//! `Element` constraint wires, which is useless to a mock consumer that has no
//! driver. There is also no standalone native Poseidon permutation — it exists
//! only as an in-circuit routine. So this module runs the *real* sponge
//! out-of-circuit by driving it through the wireless emulator
//! (`Emulator<Wireless<Always<()>, Fp>>`). The [`Sponge`] struct exists only
//! to bundle that emulator (so callers don't supply a driver on every call)
//! and to convert `Fp`↔`Element` at the boundary; every operation delegates to
//! the real sponge, and the output is bit-identical to the in-circuit sponge.
//!
//! This serves mock consumers that want *in-circuit semantics* — values
//! matching what a real ragu circuit computes — replacing the ad-hoc
//! `halo2_poseidon` use that stood in for them. It is **not** a substitute for
//! hashes defined in terms of `halo2_poseidon` itself: ragu's `PoseidonFp` is
//! a different Poseidon instantiation (width $T = 5$, rate 4) than
//! halo2_poseidon's Pasta `P128Pow5T3` (width $T = 3$, rate 2), so the two
//! disagree at every level, including the permutation. Consumers whose hashes
//! are pinned to `halo2_poseidon` output must keep using it directly.

use alloc::boxed::Box;

use ragu_arithmetic::{Cycle as _, PoseidonPermutation as _, ff::Field as _};
use ragu_core::{
    Error, Result,
    drivers::emulator::{Emulator, Wireless},
    maybe::{Always, Maybe as _},
};
use ragu_pasta::{Fp, Pasta, PoseidonFp};
use ragu_primitives::{
    Element,
    poseidon::{Sponge as InnerSponge, SpongeState as InnerState},
};

/// The wireless emulator driver used to evaluate the sponge natively. `Wire` is
/// `()` and witness values are `Always` present, so absorb/squeeze compute real
/// field values without building any constraints.
type Emu = Emulator<Wireless<Always<()>, Fp>>;

/// Number of field elements in the Poseidon state (`PoseidonFp::T`).
const STATE_LEN: usize = PoseidonFp::T;

/// An out-of-circuit Poseidon sponge over the Pallas base field `Fp`.
///
/// Mirrors [`ragu_primitives::poseidon::Sponge`] parameterized with
/// `PoseidonFp` and produces bit-identical output.
pub struct Sponge {
    emu: Emu,
    inner: InnerSponge<'static, Emu, PoseidonFp>,
}

impl Sponge {
    /// Initialize the sponge in absorb mode. Mirrors `Sponge::new`.
    pub fn new() -> Self {
        let mut emu = Emu::execute();
        let inner = InnerSponge::new(&mut emu, Pasta::circuit_poseidon(Pasta::baked()));
        Self { emu, inner }
    }

    /// Absorb one field element. Mirrors `Sponge::absorb`.
    pub fn absorb(&mut self, value: Fp) -> Result<()> {
        let element = Element::constant(&mut self.emu, value);
        self.inner.absorb(&mut self.emu, &element)
    }

    /// Squeeze one field element. Mirrors `Sponge::squeeze`.
    pub fn squeeze(&mut self) -> Result<Fp> {
        let element = self.inner.squeeze(&mut self.emu)?;
        // Wireless `Always` mode guarantees the value exists.
        Ok(*element.value().take())
    }

    /// Permute pending values and return the raw [`SpongeState`]. Mirrors
    /// `Sponge::save_state`.
    ///
    /// # Errors
    ///
    /// Fails with [`Error::Initialization`] when the sponge is already in
    /// squeeze mode or has nothing absorbed. The boxed source is the
    /// [`SaveError`](ragu_primitives::poseidon::SaveError) reported by the
    /// real sponge, which callers detect with
    /// [`Error::initialization_source`].
    pub fn save_state(self) -> Result<SpongeState> {
        let Self { mut emu, inner } = self;
        let inner_state = inner
            .save_state(&mut emu)
            .map_err(|e| Error::Initialization(Box::new(e)))?;
        let mut values = [Fp::ZERO; STATE_LEN];
        for (slot, element) in values.iter_mut().zip(inner_state.into_elements().iter()) {
            *slot = *element.value().take();
        }
        Ok(SpongeState(values))
    }

    /// Resume a sponge from a saved [`SpongeState`]. Mirrors `Sponge::resume`.
    pub fn resume(state: SpongeState) -> Self {
        let mut emu = Emu::execute();
        let elements: alloc::vec::Vec<Element<'static, Emu>> = state
            .0
            .iter()
            .map(|&value| Element::constant(&mut emu, value))
            .collect();
        let inner_state =
            InnerState::from_elements(elements.try_into().expect("STATE_LEN elements collected"));
        let inner = InnerSponge::resume(inner_state, Pasta::circuit_poseidon(Pasta::baked()));
        Self { emu, inner }
    }
}

impl Default for Sponge {
    fn default() -> Self {
        Self::new()
    }
}

/// The raw state of the Poseidon sponge (`PoseidonFp::T` field elements).
///
/// Mirrors [`ragu_primitives::poseidon::SpongeState`], holding plain `Fp`
/// values instead of in-circuit `Element`s. Save and resume sponge progress via
/// [`Sponge::save_state`] and [`Sponge::resume`].
#[derive(Clone, Copy, Debug)]
pub struct SpongeState([Fp; STATE_LEN]);

impl SpongeState {
    /// Create a [`SpongeState`] from its raw field elements.
    pub fn from_elements(values: [Fp; STATE_LEN]) -> Self {
        Self(values)
    }

    /// Consume this [`SpongeState`] and return its raw field elements.
    pub fn into_elements(self) -> [Fp; STATE_LEN] {
        self.0
    }
}
