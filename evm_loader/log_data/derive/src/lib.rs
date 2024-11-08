use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(LogData)]
pub fn derive_log_data(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = input.ident.clone();

    let implementation = gen_impl(input);

    quote! {
        impl LogData for #ident {
            fn log_data(&self) {
                #implementation
            }
        }
    }
    .into()
}

fn gen_impl(input: DeriveInput) -> proc_macro2::TokenStream {
    let syn::Data::Enum(enum_data) = input.data else {
        panic!("Only enums supported");
    };

    let branches = enum_data
        .variants
        .into_iter()
        .enumerate()
        .map(|(n, variant)| gen_branch(n, variant))
        .collect::<Vec<_>>();

    quote! {
        match self {
            #(#branches),*
        }
    }
}

fn gen_branch(n: usize, variant: syn::Variant) -> proc_macro2::TokenStream {
    let ident = variant.ident;

    let fields_count = match variant.fields {
        syn::Fields::Named(_) => panic!("Named variant fields are not supported"),
        syn::Fields::Unnamed(fields_unnamed) => fields_unnamed.unnamed.len(),
        syn::Fields::Unit => 0,
    };

    let fields = (0..fields_count)
        .map(|i| format!("arg{i}"))
        .map(|arg_string| syn::parse_str(&arg_string).unwrap())
        .collect::<Vec<syn::Ident>>();

    let fields_pattern = if fields_count > 0 {
        quote! {(#(#fields),*)}
    } else {
        quote! {}
    };

    let fields_to_bytes = fields
        .iter()
        .map(|field| quote! {&::log_data::ToBytes::to_bytes(#field)})
        .collect::<Vec<_>>();

    let comma = if fields_count > 0 {
        quote! {,}
    } else {
        quote! {}
    };

    quote! {
        Self::#ident #fields_pattern => log_data(&[
            b"ERROR",
            &::log_data::ToBytes::to_bytes(&#n),
            #(#fields_to_bytes),* #comma
            &::log_data::ToBytes::to_bytes(&self.to_string()),
        ])
    }
}
