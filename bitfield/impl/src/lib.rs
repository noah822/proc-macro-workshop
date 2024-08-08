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
                let ty = &f.ty;
                let getter_ident = format_ident!("get_{}", f.ident.as_ref().unwrap());
                let setter_ident = format_ident!("set_{}", f.ident.as_ref().unwrap());
                let prev_offset = offset.clone();
                *offset  = quote! {#offset + <#ty as Specifier>::BITS};

                Some(quote! {
                pub fn #getter_ident(&self) -> u64 {
                    let s_byte: usize = (#prev_offset) / 8;
                    let e_byte: usize = (#offset + 7) / 8;
                    let head_bits: usize = 8 - ((#prev_offset) % 8);
                    let tail_bits: usize = (#offset) % 8;

                    let mut val: u64 = 0;
                    let full_byte_range = if tail_bits != 0 {
                        val = (self.data[e_byte-1] >> (8 - tail_bits)) as u64;
                        s_byte+1..e_byte-1
                    }else {
                        s_byte+1..e_byte
                    };

                    for (i, byte_idx) in full_byte_range.rev().enumerate() {
                        val = val | (self.data[byte_idx] as u64) << (8 * (i+1));
                    }
                    if e_byte - s_byte > 1 {
                        // handle first byte
                        let first_part = self.data[s_byte] & Self::mask(head_bits) as u8;
                        val = val | (first_part as u64) << (<#ty as Specifier>::BITS - head_bits);
                    }
                    val
                }
            
                pub fn #setter_ident(&mut self, mut val: u64) {
                    let s_byte: usize = (#prev_offset) / 8;
                    let e_byte: usize = (#offset + 7) / 8;
                    let head_bits: usize = 8 - ((#prev_offset) % 8);
                    let tail_bits: usize = (#offset) % 8;

                    let full_byte_range = if tail_bits != 0 {
                        // consume remainder part                        
                        let last_part = val & Self::mask(tail_bits);
                        Self::clear_and_set(&mut self.data[e_byte-1], last_part as u8, tail_bits);
                        val >>= tail_bits;
                        s_byte+1..e_byte-1
                    }else {
                        s_byte..e_byte
                    };


                    // store val onto data in reverse order
                    val >>= tail_bits;

                    for i in full_byte_range.rev() {
                        Self::clear_and_set(&mut self.data[i], val as u8, 0);
                        val >>= 8;
                    }

                    if e_byte - s_byte > 1{
                        self.data[s_byte] &= !(Self::mask(head_bits) as u8);
                        self.data[s_byte] |= val as u8;
                    }
                }
            
            })

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

            const fn mask(num_bits: usize) -> u64 {
                (1 << num_bits) - 1
            }

            fn clear_and_set(dst: &mut u8, src: u8, offset: usize) {
                *dst = *dst & (Self::mask(offset) as u8);
                *dst = *dst | (src << offset);
            }

            pub fn display(&self) {
                for i in self.data{
                    println!("{:#08b}", i);
                }
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
