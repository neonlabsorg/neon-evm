use quote::quote;
use syn::DeriveInput;
pub fn gen_impl(input: DeriveInput) -> proc_macro2::TokenStream {
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

    // let mut fields_count = 0;
    let fields = match variant.fields {
        syn::Fields::Named(_) => panic!("Named variant fields are not supported"),
        syn::Fields::Unnamed(fields_unnamed) => {
            let fields_count = fields_unnamed.unnamed.len();
            (0..fields_count)
                .map(|i| syn::parse_str(&format!("arg{i}")).unwrap())
                .collect::<Vec<syn::Ident>>()
        }
        syn::Fields::Unit => panic!("Unit variants are not supported"),
    };


    let mut f = format!("ERROR");
    f.push_str(&format!(", {n}"));
    for field in &fields {
        f.push_str(&format!(", {{{field}}}"));
    }

    quote! {
        Self::#ident(#(#fields),*) => println!(#f)
    }

}
