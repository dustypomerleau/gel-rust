use proc_macro2::TokenStream;
use quote::quote;

use crate::attrib::ContainerAttrs;

pub fn derive_enum(
    s: &syn::ItemEnum,
    container_attrs: &ContainerAttrs,
) -> syn::Result<TokenStream> {
    let gel_protocol = container_attrs.gel_protocol_path();
    let type_name = &s.ident;
    let (impl_generics, ty_generics, _) = s.generics.split_for_impl();
    let branches = s
        .variants
        .iter()
        .map(|v| match v.fields {
            syn::Fields::Unit => {
                let attrs = crate::attrib::FieldAttrs::from_syn(&v.attrs)?;
                let name = &v.ident;
                let name_bstr = if let Some(rename) = attrs.rename {
                    syn::LitByteStr::new(rename.value().as_bytes(), rename.span())
                } else {
                    syn::LitByteStr::new(name.to_string().as_bytes(), name.span())
                };
                Ok(quote!(#name_bstr => Ok(#type_name::#name)))
            }
            _ => Err(syn::Error::new_spanned(
                &v.fields,
                "fields are not allowed in enum variants",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expanded = quote! {
        impl #impl_generics #gel_protocol::queryable::Queryable
            for #type_name #ty_generics {
            type Args = ();

            fn decode(decoder: &#gel_protocol::queryable::Decoder, _args: &(), buf: &[u8])
                -> Result<Self, #gel_protocol::errors::DecodeError>
            {
                match buf {
                    #(#branches,)*
                    _ => Err(#gel_protocol::errors::ExtraEnumValue.build()),
                }
            }
            fn check_descriptor(
                ctx: &#gel_protocol::queryable::DescriptorContext,
                type_pos: #gel_protocol::descriptors::TypePos)
                -> Result<(), #gel_protocol::queryable::DescriptorMismatch>
            {
                use #gel_protocol::descriptors::Descriptor::Enumeration;
                let desc = ctx.get(type_pos)?;
                match desc {
                    // There is no need to check the members of the enumeration
                    // because schema updates can't be perfectly synchronized
                    // to app updates. And that means that extra variants
                    // might be added and only when they are really present in
                    // data we should issue an error. Removed variants are not a
                    // problem here.
                    Enumeration(_) => Ok(()),
                    _ => Err(ctx.wrong_type(desc, "str")),
                }
            }
        }
    };
    Ok(expanded)
}
