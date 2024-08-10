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
                std::compile_error!("num variant is not power of 3");
            }
            .into();
        }
        let enum_ident = &ts.ident;
        let num_bit_required = num_variant.trailing_zeros() as usize;
        let container_ty = super::find_best_fit_ty(num_bit_required);

        let match_arms = inner_enum.variants.iter().map(|variant| {
            let variant_ident = &variant.ident;
            let (_, disc) = variant.discriminant.as_ref().unwrap();
            let lit = if let syn::Expr::Lit(syn::ExprLit { ref lit, .. }) = disc {
                lit
            } else {
                unreachable!("discrimant cannot be matched as Lit")
            };
            quote! {
                #lit => #enum_ident::#variant_ident
            }
        });
        quote! {
            impl Specifier for #enum_ident {
                const BITS: usize = #num_bit_required;
                type Container = #container_ty;
                type Target = #enum_ident;

                fn from_bit_repr(repr: Self::Container) -> Self::Target {
                    match repr {
                        #(#match_arms),*,
                        _ => unreachable!("invalid discrimant for enum")
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
