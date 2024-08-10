use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn enum_specifier(ts: TokenStream) -> TokenStream {
    let ts = parse_macro_input!(ts as DeriveInput);
    if let syn::Data::Enum(ref inner_enum) = ts.data {
        let num_variant: usize = inner_enum.variants.len();
        // check whether num_variant is power of 2
        if !num_variant.is_power_of_two() {
            return quote! {
                std::compile_error!("num variant is not power of 2");
            }
            .into();
        }
        let enum_ident = &ts.ident;
        let num_bit_required = num_variant.trailing_zeros() as usize;
        let container_ty = super::find_best_fit_ty(num_bit_required);

        // internal bit repr for each enum variant is its order within the enum
        let disc_const_idents: Vec<_> = (0..num_variant)
            .map(|i| syn::Ident::new(&format!("V{}", i), proc_macro2::Span::call_site()))
            .collect();
        let enum_variant_full_ident: Vec<_> = inner_enum
            .variants
            .iter()
            .map(|variant| {
                let variant_ident = variant.ident.clone();
                quote! {#enum_ident::#variant_ident}
            })
            .collect();
        let disc_range_check = {
            let check_stmt = enum_variant_full_ident.iter().map(|variant_ident| {
                quote! {
                    if (#variant_ident as usize) >= (1 << #num_bit_required) {
                        panic!("user specified invalid discriminater");
                    }
                }
            });
            quote! {
                const _: () = {#(#check_stmt)*};
            }
        };

        quote! {
            impl Specifier for #enum_ident {
                const BITS: usize = #num_bit_required;
                type Container = #container_ty;
                type Target = #enum_ident;

                fn from_bit_repr(repr: Self::Container) -> Self::Target {
                    // instead of parsing the discrimant from the enum token tree itself
                    // use rust internal discrimant repr for a variant

                    // const list
                    #disc_range_check
                    #(const #disc_const_idents: <#enum_ident as Specifier>::Container = #enum_variant_full_ident as <#enum_ident as Specifier>::Container;)*
                    match repr {
                        #(#disc_const_idents => #enum_variant_full_ident),*,
                        _ => unreachable!("invalid enum discrimant")
                    }
                }
                fn from_target(target: Self::Target) -> Self::Container {
                    target as Self::Container
                }

            }
        }
        .into()
    } else {
        unreachable!()
    }
}
