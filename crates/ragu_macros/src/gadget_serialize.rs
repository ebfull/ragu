use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    AngleBracketedGenericArguments, Data, DeriveInput, Error, Fields, GenericParam, Generics,
    Ident, Path, Result, parse_quote, spanned::Spanned,
};

use crate::helpers::*;

impl GenericDriver {
    fn gadget_serialize_params(&self) -> AngleBracketedGenericArguments {
        let driver_ident = &self.ident;
        let lifetime = &self.lifetime;

        parse_quote!( <#lifetime, #driver_ident> )
    }
}

pub fn derive(
    input: DeriveInput,
    ragu_core_path: Path,
    ragu_primitives_path: Path,
) -> Result<TokenStream> {
    let DeriveInput {
        ident: struct_ident,
        generics,
        data,
        ..
    } = &input;

    let driver = &generics
        .params
        .iter()
        .find_map(|p| match p {
            GenericParam::Type(ty) => ty
                .attrs
                .iter()
                .any(|a| attr_is(a, "driver"))
                .then(|| extract_generic_driver(ty)),
            _ => None,
        })
        .unwrap_or(Ok(GenericDriver::default()))?;

    // impl_generics = <'a, 'b: 'a, C: Cycle, D: Driver, const N: usize>
    // ty_generics = <'a, 'b, C, D, N>
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    if let Some(wc) = where_clause {
        return Err(Error::new(
            wc.span(),
            "GadgetSerialize derive does not yet support where clauses",
        ));
    }
    let impl_generics = {
        let mut impl_generics: Generics = parse_quote!( #impl_generics );
        impl_generics.params.iter_mut().for_each(|gp| match gp {
            GenericParam::Type(ty) if ty.ident == driver.ident => {
                // Strip out driver attribute if present
                ty.attrs.retain(|a| !attr_is(a, "driver"));
            }
            _ => {}
        });
        impl_generics
    };
    let ty_generics: AngleBracketedGenericArguments = { parse_quote!( #ty_generics ) };

    let gadget_serialize_args = driver.gadget_serialize_params();

    enum FieldType {
        Serialize,
        Skip,
    }

    let fields: Vec<(Ident, FieldType)> = match data {
        Data::Struct(s) => {
            let fields = match &s.fields {
                Fields::Named(named) => &named.named,
                _ => {
                    return Err(Error::new(
                        s.struct_token.span(),
                        "GadgetSerialize derive only works on structs with named fields",
                    ));
                }
            };

            let mut res = vec![];

            for f in fields {
                let fid = f.ident.clone().expect("fields contains only named fields");
                let is_skip = f.attrs.iter().any(|a| attr_is(a, "skip"));

                if is_skip {
                    res.push((fid, FieldType::Skip));
                } else {
                    res.push((fid, FieldType::Serialize));
                }
            }

            res
        }
        _ => {
            return Err(Error::new(
                Span::call_site(),
                "GadgetSerialize derive only works on structs",
            ));
        }
    };

    let serialize_calls = fields.iter().filter_map(|(id, ty)| match ty {
        FieldType::Serialize => Some(quote! { self.#id.serialize(dr, buf)?; }),
        FieldType::Skip => None,
    });

    let driver_ident = &driver.ident;
    let gadget_serialize_impl = {
        quote! {
            #[automatically_derived]
            impl #impl_generics #ragu_primitives_path::serialize::GadgetSerialize #gadget_serialize_args for #struct_ident #ty_generics {
                fn serialize<B: #ragu_primitives_path::serialize::Buffer #gadget_serialize_args>(&self, dr: &mut #driver_ident, buf: &mut B) -> #ragu_core_path::Result<()> {
                    #( #serialize_calls )*
                    Ok(())
                }
            }
        }
    };

    Ok(gadget_serialize_impl)
}

#[rustfmt::skip]
#[test]
fn test_gadget_serialize_derive() {
    use syn::parse_quote;

    let input: DeriveInput = parse_quote! {
        #[derive(GadgetSerialize)]
        pub struct MyGadget<'dr, #[ragu(driver)] D: Driver<'dr>> {
            field1: Element<'dr, D>,
            field2: Boolean<'dr, D>,
            #[ragu(skip)]
            phantom: ::core::marker::PhantomData<()>,
        }
    };

    let result = derive(input, parse_quote!(::ragu_core), parse_quote!(::ragu_primitives)).unwrap();

    assert_eq!(
        result.to_string(),
        quote!(
            #[automatically_derived]
            impl<'dr, D: Driver<'dr> > ::ragu_primitives::serialize::GadgetSerialize<'dr, D>
                for MyGadget<'dr, D>
            {
                fn serialize<B: ::ragu_primitives::serialize::Buffer<'dr, D> >(
                    &self,
                    dr: &mut D,
                    buf: &mut B
                ) -> ::ragu_core::Result<()> {
                    self.field1.serialize(dr, buf)?;
                    self.field2.serialize(dr, buf)?;
                    Ok(())
                }
            }
        ).to_string()
    );
}
