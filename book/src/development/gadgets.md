# Gadget System

[`Gadgets`][gadget-trait] are abstract data types that contain wires and witness data, providing stateful building blocks for composing circuits. 

**Gadget Primitives**: low-level circuit primitives including *Element* (finite field arithmetic), *Point* (elliptic curve operations), *Boolean* (constrained to zero or one), *Endoscalar* (scalar multiplication using GLV endomorphism optimization), *Poseidon Sponge* (algebraic hash function), plus utilities like *multipack* (bit packing), *multiadd* (linear combinations), and *IO* traits (serialization). 

Gadgets are implemented using the `#[derive(Gadget)]` macro with field annotations:

```rust
// Simple gadget that contains wires directly
#[derive(Gadget)]
pub struct Boolean<'dr, D: Driver<'dr>> {
    #[ragu(wire)]     // Marks a raw wire
    wire: D::Wire,
    #[ragu(value)]    // Marks witness data
    value: DriverValue<D, bool>,
}

// Composite gadget
#[derive(Gadget)]
pub struct SpongeState<'dr, D: Driver<'dr>, P: PoseidonPermutation<D::F>> {
    #[ragu(gadget)]
    values: FixedVec<Element<'dr, D>, T<D::F, P>>,  // Vector of Elements
}

```

## Traits

The derive macro generates the `Gadget<'dr, D>` trait implementation for gadgets instantiated over a specific driver `D`, using annotations to identify wires `#[ragu(wire)]`, witness values `#[ragu(value)]`, and nested gadgets `#[ragu(gadget)]`. These gadgets provide methods that allocate wires, perform constrained operations, and enforce equations (like curve constraints). Without them, we'd need to manually manage all field arithmetic, curve operations, and constraint equations using raw driver calls. 

The `GadgetExt<D>` is an extension trait that adds convenient helper methods to all types that implement `Gadget`, like `write()` to serialize the gadget to a buffer or `demote()` to strip witness data from the gadget. 

The `GadgetKind<F>` trait also provides a driver-agnostic representation of gadget types. Its `Rebind<'dr, D>` associated type enables the same gadget structure to work across different drivers, allowing gadgets to be converted between driver contexts (e.g., from `SXY` to `RX`).

## Core Properties

Gadgets have several key properties:

1. **Polymorphic** – Gadgets are parameterized by a `Driver` type, which enables writing circuit code once and reusing across different driver backends. 

2. **Fungibility** – Two instances of the same gadget *must* behave identically during circuit synthesis, which means they can't carry dynamic state or anything that would make synthesis non-deterministic. This ensures deterministic circuit synthesis and enables memoization optimizations. 

3. **Transformable** –  Gadgets define mappings between instantiations over different driver types (eg. converting `Boolean<D>` to `Boolean<D'>`) using the `GadgetKind<F>` trait. 

4. **Composable** – Gadgets can contain other gadgets as nested components.

5. **Thread safety** - Gadgets must be `Send` (movable between threads) when their driver has `Send` wires, allowing them to cross thread boundaries safely. `GadgetKind` is explicitely marked as `unsafe` because we cannot easily express this in the type system.

[gadget-trait]: ragu_core::gadgets::Gadget