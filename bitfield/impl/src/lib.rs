use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::VisitMut;
use syn::{parse_macro_input, parse_quote};

struct BitfieldVisit;

fn build_accessors(ts: &syn::ItemStruct) -> proc_macro2::TokenStream {
    let methods: Vec<_> = if let syn::Fields::Named(syn::FieldsNamed {
        named: ref fields, ..
    }) = ts.fields
    {
        fields
            .iter()
            .scan(quote! {0}, |offset, f| {
                // get this done in the easiest way, bit level ops
                let ty = &f.ty;
                let ident = f.ident.as_ref().unwrap();
                let prev_offset = offset.clone();
                *offset = quote! {#offset + <#ty as Specifier>::BITS};
                let bit_index_range = quote! {(#prev_offset)..(#offset)};
                let getter_method = {
                    let ident = format_ident!("get_{}", ident);
                    quote! {
                        pub fn #ident(&self) -> u64 {
                            let mut val = 0u64;
                            for i in #bit_index_range {
                                val <<= 1;
                                val |= self.fetch_bit(i);
                            }
                            val
                        }
                    }
                };
                let setter_method = {
                    let ident = format_ident!("set_{}", ident);
                    quote! {
                        pub fn #ident(&mut self, mut val: u64) {
                            for i in (#bit_index_range).rev() {
                                self.set_bit(i, (val & 0x1) as u8);
                                val >>= 1;
                            }
                        }
                    }
                };

                Some(quote! {#getter_method #setter_method})
            })
            .collect()
    } else {
        unreachable!()
    };

    quote! {#(#methods)*}
}

impl VisitMut for BitfieldVisit {
    fn visit_item_struct_mut(&mut self, node: &mut syn::ItemStruct) {
        // const evaluate bitsize of fields and replace them
        let array_size = if let syn::Fields::Named(syn::FieldsNamed {
            named: ref fields, ..
        }) = node.fields
        {
            let size = fields.iter().map(|f| {
                let ty = &f.ty;
                quote! {<#ty as Specifier>::BITS}
            });
            quote! {(#(#size)+*) / 8usize}
        } else {
            unreachable!()
        };

        if let syn::Fields::Named(ref mut inner) = node.fields {
            *inner = parse_quote! {
                {
                  data: [u8; #array_size],
                }
            };
        }
        node.attrs.push(parse_quote! {
            #[repr(C)]
        });
    }
}

#[proc_macro_attribute]
pub fn bitfield(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut annot_struct = parse_macro_input!(input as syn::ItemStruct);
    let struct_name = &annot_struct.ident.clone();
    let accessors = build_accessors(&annot_struct);
    BitfieldVisit.visit_item_struct_mut(&mut annot_struct);

    quote! {
        #annot_struct

        impl #struct_name{
            pub fn new() -> Self {
                Self {data: [0; 4]}
            }

            fn fetch_bit(&self, bit_index: usize) -> u64 {
                let byte_index: usize = bit_index / 8;
                let offset: usize = 7 - (bit_index % 8);
                assert!(byte_index < self.data.len());
                ((self.data[byte_index] >> offset) & 0x1) as u64
            }

            fn set_bit(&mut self, bit_index: usize, bit_val: u8) {
                let byte_index: usize = bit_index / 8;
                let offset: usize = 7 - (bit_index % 8);
                assert!(byte_index < self.data.len());
                self.data[byte_index] &= !(1 << offset);
                self.data[byte_index] |= (bit_val << offset);
            }

            pub fn display(&self) {
                let str_repr = self.data.map(|i| format!("{:08b}", i)).as_slice().join(" | ");
                println!("{}", str_repr);
            }

            #accessors
        }
    }
    .into()
}

fn syn_expr_to_usize(input: &syn::Expr) -> Option<usize> {
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(ref lit),
        ..
    }) = input
    {
        lit.base10_parse::<usize>().ok()
    } else {
        None
    }
}

#[proc_macro]
pub fn specify_bits(ts: TokenStream) -> TokenStream {
    let bit_range = parse_macro_input!(ts as syn::ExprRange);
    let start = bit_range
        .start
        .map_or(0usize, |s| syn_expr_to_usize(&s).unwrap());
    let inclusive = if let syn::RangeLimits::Closed(_) = bit_range.limits {
        true
    } else {
        false
    };
    let end = bit_range.end.and_then(|s| syn_expr_to_usize(&s)).unwrap();
    let bit_range = std::ops::Range {
        start,
        end: if inclusive { end + 1 } else { end },
    };
    let trait_impl = bit_range.map(|i| {
        let ident = syn::Ident::new(&format!("{}{}", "B", i), proc_macro2::Span::call_site());
        quote! {
            pub enum #ident {}
            impl Specifier for #ident {
                const BITS: usize = #i;
            }
        }
    });
    quote! {
        #(#trait_impl)*
    }
    .into()
}
