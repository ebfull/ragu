use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenTree};
use quote::format_ident;
use syn::{
    Attribute, Error, GenericArgument, GenericParam, Ident, Lifetime, Meta, Path, PathArguments,
    Result, Type, TypeParam, TypeParamBound, TypePath, parse_quote, spanned::Spanned,
};

pub fn ragu_core_path() -> Result<Path> {
    Ok(match (crate_name("ragu_core"), crate_name("ragu")) {
        (Ok(FoundCrate::Itself), _) => parse_quote! { ::ragu_core },
        (_, Ok(FoundCrate::Itself)) => parse_quote! { ::ragu },
        (Ok(FoundCrate::Name(name)), _) | (Err(_), Ok(FoundCrate::Name(name))) => {
            let name: Ident = format_ident!("{}", name);
            parse_quote! { ::#name }
        }
        _ => {
            return Err(Error::new(
                Span::call_site(),
                "Failed to find ragu/ragu_core crate. Ensure it is included in your Cargo.toml.",
            ));
        }
    })
}

pub fn ragu_primitives_path() -> Result<Path> {
    Ok(match (crate_name("ragu_primitives"), crate_name("ragu")) {
        (Ok(FoundCrate::Itself), _) => parse_quote! { ::ragu_primitives },
        (_, Ok(FoundCrate::Itself)) => parse_quote! { ::ragu::primitives },
        (Ok(FoundCrate::Name(name)), _) => {
            let name: Ident = format_ident!("{}", name);
            parse_quote! { ::#name }
        }
        (_, Ok(FoundCrate::Name(name))) => {
            let name: Ident = format_ident!("{}", name);
            parse_quote! { ::#name::primitives }
        }
        _ => {
            return Err(Error::new(
                Span::call_site(),
                "Failed to find ragu/ragu_primitives crate. Ensure it is included in your Cargo.toml.",
            ));
        }
    })
}

pub fn attr_is(attr: &Attribute, needle: &str) -> bool {
    if !attr.path().is_ident("ragu") {
        return false;
    }
    match &attr.meta {
        Meta::List(list) => list.tokens.clone().into_iter().any(|tt| match tt {
            TokenTree::Ident(ref ident) => ident == needle,
            _ => false,
        }),
        _ => false,
    }
}

#[test]
fn test_attr_is() {
    let attr: Attribute = parse_quote!(#[ragu(driver)]);
    assert!(attr_is(&attr, "driver"));
    assert!(!attr_is(&attr, "not_driver"));

    let attr: Attribute = parse_quote!(#[ragu(not_driver)]);
    assert!(!attr_is(&attr, "driver"));
    assert!(attr_is(&attr, "not_driver"));

    let attr: Attribute = parse_quote!(#[ragu]);
    assert!(!attr_is(&attr, "driver"));

    let attr: Attribute = parse_quote!(#[not_ragu(driver)]);
    assert!(!attr_is(&attr, "driver"));
}

pub struct GenericDriver {
    pub ident: Ident,
    pub lifetime: Lifetime,
}

impl Default for GenericDriver {
    fn default() -> Self {
        Self {
            ident: format_ident!("D"),
            lifetime: Lifetime::new("'dr", Span::call_site()),
        }
    }
}

/// Extracts the identifiers D and 'dr from a TypeParam of the form `D: path::to::Driver<'dr>`.
pub fn extract_generic_driver(param: &TypeParam) -> Result<GenericDriver> {
    for bound in &param.bounds {
        if let TypeParamBound::Trait(bound) = bound {
            if let Some(seg) = bound.path.segments.last() {
                if seg.ident != "Driver" {
                    continue;
                }
                if let PathArguments::AngleBracketed(args) = &seg.arguments {
                    let lifetimes = args
                        .args
                        .iter()
                        .filter_map(|arg| {
                            if let GenericArgument::Lifetime(lt) = arg {
                                Some(lt.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    if lifetimes.len() == 1 {
                        return Ok(GenericDriver {
                            ident: param.ident.clone(),
                            lifetime: lifetimes[0].clone(),
                        });
                    } else {
                        return Err(Error::new(args.span(), "expected a single lifetime bound"));
                    }
                } else {
                    return Err(Error::new(seg.ident.span(), "expected a lifetime bound"));
                }
            }
        }
    }

    Err(Error::new(param.span(), "expected a Driver<'dr> bound"))
}

pub trait Substitution {
    fn substitute(&mut self, driver_id: &Ident, driverfield_ident: &Ident);
}

impl Substitution for TypePath {
    fn substitute(&mut self, driver_id: &Ident, driverfield_ident: &Ident) {
        if self.qself.is_none() && self.path.segments.len() == 2 {
            let segs = &self.path.segments;
            if segs[0].ident == *driver_id && segs[1].ident == "F" {
                *self = parse_quote!(#driverfield_ident);
                return;
            }
        }

        for seg in &mut self.path.segments {
            if let PathArguments::AngleBracketed(ab) = &mut seg.arguments {
                for arg in ab.args.iter_mut() {
                    match arg {
                        GenericArgument::Type(t) => {
                            t.substitute(driver_id, driverfield_ident);
                        }
                        GenericArgument::Constraint(constraint) => {
                            constraint.bounds.iter_mut().for_each(|bound| {
                                bound.substitute(driver_id, driverfield_ident);
                            });
                        }
                        GenericArgument::AssocType(assoc_type) => {
                            assoc_type.ty.substitute(driver_id, driverfield_ident);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

impl Substitution for Type {
    fn substitute(&mut self, driver_id: &Ident, driverfield_ident: &Ident) {
        match self {
            Type::Path(type_path) => {
                type_path.substitute(driver_id, driverfield_ident);
            }
            Type::Tuple(tuple) => {
                for elem in &mut tuple.elems {
                    elem.substitute(driver_id, driverfield_ident);
                }
            }
            _ => {}
        }
    }
}

impl Substitution for TypeParamBound {
    fn substitute(&mut self, driver_id: &Ident, driverfield_ident: &Ident) {
        if let TypeParamBound::Trait(trait_bound) = self {
            for seg in &mut trait_bound.path.segments {
                if let syn::PathArguments::AngleBracketed(ab) = &mut seg.arguments {
                    for arg in ab.args.iter_mut() {
                        match arg {
                            GenericArgument::Type(t) => {
                                t.substitute(driver_id, driverfield_ident);
                            }
                            GenericArgument::Constraint(constraint) => {
                                constraint.bounds.iter_mut().for_each(|b| {
                                    b.substitute(driver_id, driverfield_ident);
                                });
                            }
                            GenericArgument::AssocType(assoc_type) => {
                                assoc_type.ty.substitute(driver_id, driverfield_ident);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

pub fn replace_driver_field_in_generic_param(
    param: &mut syn::GenericParam,
    driver_id: &syn::Ident,
    driverfield_ident: &syn::Ident,
) {
    if let GenericParam::Type(TypeParam {
        bounds, default, ..
    }) = param
    {
        for bound in bounds.iter_mut() {
            bound.substitute(driver_id, driverfield_ident);
        }
        if let Some(default_ty) = default {
            default_ty.substitute(driver_id, driverfield_ident);
        }
    }
}
