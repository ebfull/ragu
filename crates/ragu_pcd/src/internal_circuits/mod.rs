use arithmetic::Cycle;
use ragu_circuits::{
    mesh::MeshBuilder,
    polynomials::Rank,
    staging::{StageExt, StagedCircuit},
};
use ragu_core::Result;

pub mod c;
pub mod dummy;
pub mod stages;
pub mod unified;

const DUMMY_CIRCUIT_ID: usize = 0;
const C_CIRCUIT_ID: usize = 1;
const NATIVE_PREAMBLE_CIRCUIT_ID: usize = 2;
const C_STAGED_ID: usize = 3;

pub fn index(num_application_steps: usize, internal_index: usize) -> usize {
    num_application_steps + super::step::NUM_INTERNAL_STEPS + internal_index
}

pub fn register_all<'params, C: Cycle, R: Rank>(
    mesh: MeshBuilder<'params, C::CircuitField, R>,
    params: &'params C,
) -> Result<MeshBuilder<'params, C::CircuitField, R>> {
    let mesh = mesh.register_circuit(dummy::Circuit)?;
    let mesh = mesh.register_circuit(c::Circuit::<C, R>::new(params.circuit_poseidon()))?;
    let mesh =
        mesh.register_circuit_object(stages::native::preamble::Stage::<C, R>::into_object()?)?;
    let mesh = mesh
        .register_circuit_object(
            <c::Circuit<C, R> as StagedCircuit<C::CircuitField, R>>::Final::final_into_object()?,
        )?;
    Ok(mesh)
}
