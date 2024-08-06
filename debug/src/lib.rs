use proc_macro::TokenStream;
use quote::quote;
use std::boxed::Box;
use std::collections::HashSet;
use syn::{parse_macro_input, DeriveInput};

fn extract_last_ty(syn_ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(ref path) = syn_ty {
        let ty = path.path.segments.last()?;
        Some(ty.ident.to_string())
    } else {
        None
    }
}

fn extract_first_generic(generic_list: &syn::Generics) -> Option<String> {
    let params = &generic_list.params;
    params.first().and_then(|first_t| {
        if let syn::GenericParam::Type(ref first_t) = first_t {
            Some(first_t.ident.to_string())
        } else {
            None
        }
    })
}

fn path_to_ident_set(path: &syn::Path) -> HashSet<String> {
    let mut token_set = HashSet::new();
    for seg in path.segments.iter() {
        token_set.insert(seg.ident.to_string());
    }
    token_set
}

fn try_extract_inner_ty(syn_ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(ref path) = syn_ty {
        let ty = path.path.segments.last()?;
        return match ty.arguments {
            syn::PathArguments::None => Some(syn_ty),
            syn::PathArguments::AngleBracketed(ref inner_ty) => {
                if let syn::GenericArgument::Type(first_ty) = inner_ty.args.first()? {
                    // recurise on this
                    try_extract_inner_ty(first_ty)
                } else {
                    None
                }
            }
            _ => None,
        };
    }
    None
}

// return key value pair of an attribute
// #[debug(bond = "xxx")]
fn parse_outter_attr(attrs: &syn::Attribute) -> Option<(syn::Ident, syn::LitStr)> {
    assert!(attrs.path().is_ident("debug"));
    attrs
        .parse_args_with(|stream: syn::parse::ParseStream| {
            let key: syn::Ident = stream.parse()?;
            let _: syn::Token![=] = stream.parse()?;
            let value: syn::LitStr = stream.parse()?;
            Ok((key, value))
        })
        .ok()
}

#[proc_macro_derive(CustomDebug, attributes(debug))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;

    let struct_name_str = struct_name.to_string();
    let mut has_phantom = false;
    let mut tys_to_bound: HashSet<syn::TypePath> = HashSet::new();

    let generics = &input.generics;
    let has_generic = generics.type_params().next().is_some();
    let generic_ident = extract_first_generic(generics).unwrap_or(String::from(""));

    // try inspect outter attr
    let outter_attrs = input.attrs;
    let trait_bound_quick_path = if outter_attrs.len() > 0 {
        assert_eq!(outter_attrs.len(), 1);
        parse_outter_attr(&outter_attrs[0])
    } else {
        None
    };

    let debug_fields = if let syn::Data::Struct(st) = input.data {
        st.fields.into_iter().map(|f| {
            let ident = f.ident.unwrap();
            if let Some(ty) = extract_last_ty(&f.ty) {
                if ty == String::from("PhantomData") {
                    has_phantom = true;
                } else {
                    try_extract_inner_ty(&f.ty).map(|ty_arg| {
                        if let syn::Type::Path(ty_path) = ty_arg {
                            if has_generic
                                && path_to_ident_set(&ty_path.path).contains(&generic_ident)
                            {
                                tys_to_bound.insert(ty_path.clone());
                            }
                        }
                    });
                }
            }
            let mut fmt_str = String::from("{:?}");

            // process attribute if there is one attached
            if f.attrs.len() > 0 {
                assert_eq!(f.attrs.len(), 1);
                fmt_str = if let syn::Attribute {
                    meta: syn::Meta::NameValue(ref fmt_attr),
                    ..
                } = f.attrs[0]
                {
                    assert!(fmt_attr.path.is_ident("debug"));
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(ref fmt_str),
                        ..
                    }) = fmt_attr.value
                    {
                        fmt_str.value()
                    } else {
                        unreachable!()
                    }
                } else {
                    unreachable!()
                };
            }

            let ident_str = ident.to_string();
            quote! {
                .field(&String::from(#ident_str), &format_args!(#fmt_str, &self.#ident))
            }
        })
    } else {
        unreachable!()
    };

    let fmt_fn_impl = quote! {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error>{
            f.debug_struct(&String::from(#struct_name_str))
             #(#debug_fields)*
             .finish();
            Ok(())
        }
    };

    let ret_stream = if has_generic {
        let (impl_generics, ty_generics, _) = generics.split_for_impl();
        if !tys_to_bound.is_empty() {
            // find actual type to bound
            let bounded_generic_tys: Box<dyn Iterator<Item = proc_macro2::TokenStream>> =
                if let Some((config_key, config_trait_bound)) = trait_bound_quick_path {
                    assert_eq!(&config_key.to_string(), "bound");
                    let config_trait_bound =
                        syn::parse_str::<syn::WherePredicate>(&config_trait_bound.value())
                            .expect("can not parse it as TypeParam");
                    Box::new(std::iter::once(quote! {#config_trait_bound}))
                } else {
                    Box::new(tys_to_bound.iter().map(|ty| {
                        quote! { #ty: std::fmt::Debug }
                    }))
                };
            quote! {
                impl #impl_generics std::fmt::Debug for #struct_name #ty_generics
                where #(#bounded_generic_tys),*
                {
                    #fmt_fn_impl
                }
            }
        } else {
            quote! {
                impl #impl_generics std::fmt::Debug for #struct_name #ty_generics
                {
                    #fmt_fn_impl
                }
            }
        }
    } else {
        quote! {
            impl std::fmt::Debug for #struct_name {
                #fmt_fn_impl
            }
        }
    };
    ret_stream.into()
}
