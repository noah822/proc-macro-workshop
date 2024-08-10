use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::VisitMut;
use syn::{parse_macro_input, parse_quote};

mod specifier;

static WIDTH_PTYPE: [usize; 5] = [8, 16, 32, 64, 128];
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
                        pub fn #ident(&self) -> <#ty as Specifier>::Target{
                            let mut val = 0u64;
                            for i in #bit_index_range {
                                val <<= 1;
                                val |= self.fetch_bit(i);
                            }
                            let repr = val as <#ty as Specifier>::Container;
                            <#ty as Specifier>::from_bit_repr(repr)
                        }
                    }
                };
                let setter_method = {
                    let ident = format_ident!("set_{}", ident);
                    quote! {
                        pub fn #ident(&mut self, val: <#ty as Specifier>::Target) {
                            let mut val = <#ty as Specifier>::from_target(val);
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

fn get_total_bit_width(ts: &syn::ItemStruct) -> proc_macro2::TokenStream {
    if let syn::Fields::Named(syn::FieldsNamed {
        named: ref fields, ..
    }) = ts.fields
    {
        fields.iter().fold(quote! {0}, |acc, f| {
            let ty = &f.ty;
            quote! {#acc + <#ty as Specifier>::BITS}
        })
    } else {
        unreachable!()
    }
}

fn sanity_check(ts: &syn::ItemStruct) -> proc_macro2::TokenStream {
    let bit_width = get_total_bit_width(&ts);
    let bit_tag_check = if let syn::Fields::Named(syn::FieldsNamed {
        named: ref fields, ..
    }) = ts.fields
    {
        fields.iter().filter_map(|f| {
            if f.attrs.len() > 0 {
                assert_eq!(f.attrs.len(), 1);
                let bit_tag = &f.attrs[0];
                let ty = &f.ty;
                assert!(bit_tag.path().is_ident("bits"));
                if let syn::Meta::NameValue(syn::MetaNameValue { ref value, .. }) = bit_tag.meta {
                    let tagged_width = syn_expr_to_usize(&value).unwrap();
                    let check = quote! {
                        if #tagged_width != (<#ty as Specifier>::BITS as usize) {
                            panic!("tagged bit does not align with the underlying bit width");
                        }
                    };
                    return Some(check);
                } else {
                    unreachable!("invalid inner `bitfield` tag format")
                }
            }
            None
        })
    } else {
        unreachable!()
    };

    quote! {
        const _: () = {
            if (#bit_width) % 8 != 0 {panic!("sum of bit width is not divisive by 8");}
        };
        const _: () = {
            #(#bit_tag_check)*
        };
    }
}



/// blanket impl for inner #[bit = xxx] attribute
// #[proc_macro_attribute]
// pub fn bits(_: TokenStream, input: TokenStream) -> TokenStream {input}

#[proc_macro_attribute]
pub fn bitfield(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut annot_struct = parse_macro_input!(input as syn::ItemStruct);
    let struct_name = &annot_struct.ident.clone();

    let accessors = build_accessors(&annot_struct);
    let bit_width = get_total_bit_width(&annot_struct);

    // check sanity of the bitfield struct
    // 1. sum of bit width
    // 2. bit annotation aligns with the actual bit width constant in Specifier
    let checker = sanity_check(&annot_struct);
    BitfieldVisit.visit_item_struct_mut(&mut annot_struct);

    quote! {
        #checker
        #annot_struct
        impl #struct_name{
            pub fn new() -> Self {
                Self {data: [0; (#bit_width)/8]}
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

fn find_best_fit_ty(num_bit: usize) -> syn::Ident {
    let type_suffix = {
        let idx = WIDTH_PTYPE
            .as_slice()
            .iter()
            .position(|width| *width >= num_bit)
            .unwrap();
        WIDTH_PTYPE[idx]
    };
    syn::Ident::new(&format!("u{}", type_suffix), proc_macro2::Span::call_site())
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
        let container_ty = find_best_fit_ty(i);
        quote! {
            pub enum #ident {}
            impl Specifier for #ident {
                const BITS: usize = #i;
                type Container = #container_ty;
                type Target = #container_ty;

                fn from_bit_repr(repr: Self::Container) -> Self::Target {
                    repr as Self::Target
                }

                fn from_target(target: Self::Target) -> Self::Container {
                    target as Self::Container
                }
            }
        }
    });
    quote! {
        #(#trait_impl)*
    }
    .into()
}

/// BitfieldSpecifier
#[proc_macro_derive(BitfieldSpecifier)]
pub fn enum_specifier(ts: TokenStream) -> TokenStream {
    specifier::enum_specifier(ts)
}
