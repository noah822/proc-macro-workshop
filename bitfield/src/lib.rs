// Crates that have the "proc-macro" crate type are only allowed to export
// procedural macros. So we cannot have one crate that defines procedural macros
// alongside other types of public APIs like traits and structs.
//
// For this project we are going to need a #[bitfield] macro but also a trait
// and some structs. We solve this by defining the trait and structs in this
// crate, defining the attribute macro in a separate bitfield-impl crate, and
// then re-exporting the macro from this crate so that users only have one crate
// that they need to import.
//
// From the perspective of a user of this crate, they get all the necessary APIs
// (macro, trait, struct) through the one bitfield crate.


/// TODO: figure out how to better report error with correct span instead of just panic

#[allow(unused_imports)]
pub use bitfield_impl::{bitfield, BitfieldSpecifier};

pub trait Specifier {
    const BITS: usize;
    // minimal rust primitive type that contains the internal bit repr
    type Container;
    // target type the contain wants to coerse to
    type Target;

    fn from_bit_repr(repr: Self::Container) -> Self::Target;
    fn from_target(target: Self::Target) -> Self::Container;
}

bitfield_impl::specify_bits!(0..=64);

/// blanket impl for some rust primitives
impl Specifier for bool {
    const BITS: usize = 1;
    type Container = u8;
    type Target = bool;

    fn from_bit_repr(repr: Self::Container) -> Self::Target {
        match repr {
            0 => false,
            1 => true,
            _ => unreachable!("invalid internal repr for `bool`"),
        }
    }

    fn from_target(target: Self::Target) -> Self::Container {
        if target {
            1
        } else {
            0
        }
    }
}
