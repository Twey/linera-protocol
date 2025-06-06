extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;

#[derive(Debug, deluxe::ExtractAttributes)]
#[deluxe(attributes(view))]
struct StructAttrs {
    context: Option<syn::Type>,
    default: Option<syn::Type>,
}

#[derive(Debug, deluxe::ParseAttributes)]
#[deluxe(attributes(view))]
struct FieldAttrs {
    #[deluxe(default)]
    skip: bool,
    default: Option<DefaultExpr>,
}

#[derive(Debug)]
struct DefaultExpr(syn::Expr);

impl deluxe::ParseMetaItem for DefaultExpr {
    #[inline]
    fn parse_meta_item(input: syn::parse::ParseStream, _mode: deluxe::ParseMode) -> deluxe::Result<Self> {
        Ok(Self(input.parse::<syn::Expr>()?))
    }

    #[inline]
    fn parse_meta_item_flag(_: proc_macro2::Span) -> deluxe::Result<Self> {
        Ok(Self(syn::parse_quote! { ::core::default::Default::default() }))
    }
}

#[proc_macro_derive(View, attributes(view))]
pub fn derive_view(input: TokenStream) -> TokenStream {
    let mut input = syn::parse_macro_input!(input as syn::DeriveInput);
    let struct_attrs: StructAttrs = deluxe::extract_attributes(&mut input).unwrap();
    let syn::Data::Struct(struct_) = input.data else { panic!("no!") };
    let mut attrs = std::collections::HashMap::new();
    for mut field in struct_.fields.into_iter() {
        let field_attrs: FieldAttrs = deluxe::parse_attributes(&field).unwrap();
        attrs.insert(field.ident, field_attrs);
    }
    panic!("{:#?}", attrs);
    quote! { }.into()
}
