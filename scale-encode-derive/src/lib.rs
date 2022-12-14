use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, punctuated::Punctuated};

const ATTR_NAME: &str = "encode_as_type";

/// The `EncodeAsType` derive macro can be used to implement `EncodeAsType`
/// on structs and enums whose fields all implement `EncodeAsType`.
///
/// # Example
///
/// ```rust
/// use scale_encode as alt_path;
/// use scale_encode::EncodeAsType
///
/// #[derive(EncodeAsType)]
/// #[encode_as_type(trait_bounds = "", crate_path = "alt_path")]
/// struct Foo<T> {
///    a: u64,
///    b: bool,
///    c: std::marker::PhantomData<T>
/// }
/// ```
///
/// # Attributes
///
/// - `#[encode_as_type(crate_path = "::path::to::scale_encode")]`:
///   By default, the macro expects `scale_encode` to be a top level dependency,
///   available as `::scale_encode`. If this is not the case, you can provide the
///   crate path here.
/// - `#[encode_as_type(type_path = "::path::to::ForeignType")]`:
///   By default, the macro will generate an impl for the type it's given. If you'd like
///   to use the given type as a template to generate an impl for some foreign type, you
///   can pass a path to the foreign type to generate the impl for here. This is mainly
///   used with the `#[encode_as_type]` attribute macro (and indeed is necessary there).
/// - `#[encode_as_type(trait_bounds = "T: Foo, U::Input: EncodeAsType")]`:
///   By default, for each generate type parameter, the macro will add trait bounds such
///   that these type parameters must implement `EncodeAsType` too. You can override this
///   behaviour and provide your own trait bounds instead using this option.
///
/// # Limitations
///
/// The generated `EncodeAsType` impls currently support a maximum of 32 fields in the
/// struct or variant; if you exceed this number you'll hit a compile error.
#[proc_macro_derive(EncodeAsType, attributes(encode_as_type))]
pub fn derive_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // parse top level attrs.
    let attrs = match TopLevelAttrs::parse(&input.attrs) {
        Ok(attrs) => attrs,
        Err(e) => return e.write_errors().into()
    };

    derive_with_attrs(attrs, input).into()
}

/// The `#[encode_as_type]` attribute macro is similar in what it generates to the `#[derive(EncodeAsType)]`
/// macro. The main difference is that this macro is used to generate an `EncodeAsType` implementation on
/// some foreign type. As such, the `#[encode_as_type(type_path = "::path::to::ForeignType")]` attribute
/// is mandatory in order to specify the foreign type to generate the impl for.
///
/// The struct or variant that this `#[encode_as_type]` attribute macro is placed on is used simply as a
/// template to generate the correct impl, and so should mirror the foreign type. It will disappear from
/// the code otherwise.
#[proc_macro_attribute]
pub fn encode_as_type(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    // parse top level attrs.
    let attrs = match TopLevelAttrs::parse(&input.attrs) {
        Ok(attrs) => attrs,
        Err(e) => return e.write_errors().into()
    };

    // Require a type_path to be given for the attr macro.
    if attrs.type_path.is_none() {
        return syn::Error::new_spanned(
            TokenStream2::from(attr),
            "The #[encode_as_type] attribute macro requires that #[encode_as_type(type_path = \"::path::to::Type\")] is given"
        ).into_compile_error().into()
    }

    derive_with_attrs(attrs, input).into()
}

fn derive_with_attrs(attrs: TopLevelAttrs, input: DeriveInput) -> TokenStream2 {
    // what type is the derive macro declared on?
    match &input.data {
        syn::Data::Enum(details) => {
            generate_enum_impl(attrs, &input, details).into()
        },
        syn::Data::Struct(details) => {
            generate_struct_impl(attrs, &input, details).into()
        },
        syn::Data::Union(_) => {
            syn::Error::new(input.ident.span(), "Unions are not supported by the EncodeAsType macro")
                .into_compile_error()
                .into()
        }
    }
}

fn generate_enum_impl(attrs: TopLevelAttrs, input: &DeriveInput, details: &syn::DataEnum) -> TokenStream2 {
    let path_to_scale_encode = &attrs.crate_path;
    let default_path_to_type = input.ident.clone().into();
    let path_to_type = attrs.type_path.as_ref().unwrap_or(&default_path_to_type);
    let (impl_generics, ty_generics, where_clause) = handle_generics(&attrs, &input.generics);

    // For each variant we want to spit out a match arm.
    let match_arms = details.variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        let variant_name_str = variant_name.to_string();

        let (matcher, composite) = fields_to_matcher_and_composite(&path_to_scale_encode, &variant.fields);
        quote!(
            Self::#variant_name #matcher => {
                #path_to_scale_encode::utils::Variant { name: #variant_name_str, fields: #composite }
                    .encode_as_type_to(
                        __encode_as_type_type_id,
                        __encode_as_type_types,
                        __encode_as_type_context,
                        __encode_as_type_out
                    )
            }
        )
    });

    quote!(
        impl #impl_generics #path_to_scale_encode::EncodeAsType for #path_to_type #ty_generics #where_clause {
            fn encode_as_type_to(
                &self,
                // long variable names to prevent conflict with struct field names:
                __encode_as_type_type_id: u32,
                __encode_as_type_types: &#path_to_scale_encode::utils::PortableRegistry,
                __encode_as_type_context: #path_to_scale_encode::Context,
                __encode_as_type_out: &mut Vec<u8>
            ) -> Result<(), #path_to_scale_encode::Error> {
                match self {
                    #( #match_arms ),*
                }
            }
        }
    )
}

fn generate_struct_impl(attrs: TopLevelAttrs, input: &DeriveInput, details: &syn::DataStruct) -> TokenStream2 {
    let path_to_scale_encode = &attrs.crate_path;
    let default_path_to_type = input.ident.clone().into();
    let path_to_type = attrs.type_path.as_ref().unwrap_or(&default_path_to_type);
    let (impl_generics, ty_generics, where_clause) = handle_generics(&attrs, &input.generics);

    let (matcher, composite) = fields_to_matcher_and_composite(&path_to_scale_encode, &details.fields);

    quote!(
        impl #impl_generics #path_to_scale_encode::EncodeAsType for #path_to_type #ty_generics #where_clause {
            fn encode_as_type_to(
                &self,
                // long variable names to prevent conflict with struct field names:
                __encode_as_type_type_id: u32,
                __encode_as_type_types: &#path_to_scale_encode::utils::PortableRegistry,
                __encode_as_type_context: #path_to_scale_encode::Context,
                __encode_as_type_out: &mut Vec<u8>
            ) -> Result<(), #path_to_scale_encode::Error> {
                let #path_to_type #matcher = self;
                #composite.encode_as_type_to(
                    __encode_as_type_type_id,
                    __encode_as_type_types,
                    __encode_as_type_context,
                    __encode_as_type_out
                )
            }
        }
    )
}

fn handle_generics<'a>(attrs: &TopLevelAttrs, generics: &'a syn::Generics) -> (syn::ImplGenerics<'a>, syn::TypeGenerics<'a>, syn::WhereClause) {
    let path_to_crate = &attrs.crate_path;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut where_clause = where_clause.cloned().unwrap_or(syn::parse_quote!(where));

    if let Some(where_predicates) = &attrs.trait_bounds {
        // if custom trait bounds are given, append those to the where clause.
        where_clause.predicates.extend(where_predicates.clone());

    } else {
        // else, append our default EncodeAsType bounds to the where clause.
        for param in generics.type_params() {
            let ty = &param.ident;
            where_clause.predicates.push(syn::parse_quote!(#ty: #path_to_crate::EncodeAsType))
        }
    }

    (impl_generics, ty_generics, where_clause)
}

fn fields_to_matcher_and_composite(path_to_scale_encode: &syn::Path, fields: &syn::Fields) -> (TokenStream2, TokenStream2) {
    match fields {
        syn::Fields::Named(fields) => {
            let match_body = fields.named
                .iter()
                .map(|f| {
                    let field_name = &f.ident;
                    quote!(#field_name)
                });
            let tuple_body = fields.named
                .iter()
                .map(|f| {
                    let field_name_str = f.ident.as_ref().unwrap().to_string();
                    let field_name = &f.ident;
                    quote!((Some(#field_name_str), #field_name))
                });
            // add a closing comma if one field to make sure that the thing we generate
            // is still seen as a tuple and not just brackets around an item.
            let closing_comma = if fields.named.len() == 1 {
                quote!(,)
            } else {
                quote!()
            };
            (
                quote!({#( #match_body ),*}),
                quote!(#path_to_scale_encode::utils::Composite((#( #tuple_body ),* #closing_comma)))
            )
        },
        syn::Fields::Unnamed(fields) => {
            let field_idents: Vec<syn::Ident> = fields.unnamed
                .iter()
                .enumerate()
                .map(|(idx, _)| format_ident!("_{idx}"))
                .collect();
            let match_body = field_idents
                .iter()
                .map(|i| quote!(#i));
            let tuple_body = field_idents
                .iter()
                .map(|i| {
                    quote!((None as Option<&'static str>, #i))
                });
            // add a closing comma if one field to make sure that the thing we generate
            // is still seen as a tuple and not just brackets around an item.
            let closing_comma = if fields.unnamed.len() == 1 {
                quote!(,)
            } else {
                quote!()
            };
            (
                quote!((#( #match_body ),*)),
                quote!(#path_to_scale_encode::utils::Composite((#( #tuple_body ),* #closing_comma)))
            )
        },
        syn::Fields::Unit => {
            (
                quote!(),
                quote!(#path_to_scale_encode::utils::Composite(()))
            )
        }
    }
}

struct TopLevelAttrs {
    // path to the scale_encode crate, in case it's not a top level dependency.
    crate_path: syn::Path,
    // path to the type that we're generating the impl for. None to use the existing item name
    type_path: Option<syn::Path>,
    // allow custom trait bounds to be used instead of the defaults.
    trait_bounds: Option<Punctuated<syn::WherePredicate, syn::Token!(,)>>
}

impl TopLevelAttrs {
    fn parse(attrs: &[syn::Attribute]) -> darling::Result<Self> {
        use darling::FromMeta;

        #[derive(FromMeta)]
        struct TopLevelAttrsInner {
            #[darling(default)]
            crate_path: Option<syn::Path>,
            #[darling(default)]
            type_path: Option<syn::Path>,
            #[darling(default)]
            trait_bounds: Option<Punctuated<syn::WherePredicate, syn::Token!(,)>>
        }

        let mut res = TopLevelAttrs {
            crate_path: syn::parse_quote!(::scale_encode),
            type_path: None,
            trait_bounds: None
        };

        // look at each top level attr. parse any for encode_as_type.
        for attr in attrs {
            if !attr.path.is_ident(ATTR_NAME) {
                continue
            }
            let meta = attr.parse_meta()?;
            let parsed_attrs = TopLevelAttrsInner::from_meta(&meta)?;

            res.type_path = parsed_attrs.type_path;
            res.trait_bounds = parsed_attrs.trait_bounds;
            if let Some(crate_path) = parsed_attrs.crate_path {
                res.crate_path = crate_path;
            }
        }

        Ok(res)
    }
}