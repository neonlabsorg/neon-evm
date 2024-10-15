#![deny(warnings)]
#![deny(clippy::all, clippy::pedantic, clippy::nursery)]

mod config_parser;

use std::collections::BTreeMap;

use config_parser::{CommonConfig, NetSpecificConfig};
use proc_macro::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{
    parse_macro_input, Data::Struct, DataStruct, DeriveInput, Expr, Fields::Named, FieldsNamed,
    GenericArgument, Ident, LitStr, PathArguments, Result, Token, Type, TypePath, TypeTuple,
};

use quote::quote;

extern crate proc_macro;

struct ElfParamInput {
    name: Ident,
    _separator: Token![,],
    value: Expr,
}

impl Parse for ElfParamInput {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            _separator: input.parse()?,
            value: input.parse()?,
        })
    }
}

#[proc_macro]
pub fn neon_elf_param(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as ElfParamInput);

    let name = input.name;
    let value = input.value;

    quote! {
        #[no_mangle]
        #[used]
        #[doc(hidden)]
        pub static #name: [u8; #value.len()] = {
            #[allow(clippy::string_lit_as_bytes)]
            let bytes: &[u8] = #value.as_bytes();

            let mut array = [0; #value.len()];
            let mut i = 0;
            while i < #value.len() {
                array[i] = bytes[i];
                i += 1;
            }
            array
        };
    }
    .into()
}

/// # Panics
/// Panic at compile time if config file is not correct
#[proc_macro]
pub fn net_specific_config_parser(tokens: TokenStream) -> TokenStream {
    let NetSpecificConfig {
        program_id,
        neon_chain_id,
        neon_token_mint,
        no_update_tracking_owners,
        mut chains,
    } = parse_macro_input!(tokens as NetSpecificConfig);

    let mut no_update_tracking_owners: Vec<Vec<u8>> = no_update_tracking_owners
        .iter()
        .map(|key| bs58::decode(key).into_vec().unwrap())
        .collect();

    no_update_tracking_owners.sort_unstable();
    let no_update_tracking_owners_len = no_update_tracking_owners.len();

    chains.sort_unstable_by_key(|c| c.id);
    let chains_len = chains.len();

    let chain_ids = chains.iter().map(|c| c.id).collect::<Vec<_>>();
    let chain_names = chains.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let chain_tokens = chains
        .iter()
        .map(|c| bs58::decode(&c.token).into_vec().unwrap())
        .collect::<Vec<_>>();

    let neon_chain_id_str = neon_chain_id.to_string();

    quote! {
        pub const PROGRAM_ID: solana_program::pubkey::Pubkey = solana_program::pubkey!(#program_id);
        pub const DEFAULT_CHAIN_ID: u64 = #neon_chain_id;

        neon_elf_param!(NEON_CHAIN_ID, #neon_chain_id_str);
        neon_elf_param!(NEON_TOKEN_MINT, #neon_token_mint);

        pub const NO_UPDATE_TRACKING_OWNERS: [::solana_program::pubkey::Pubkey; #no_update_tracking_owners_len] = [
            #(::solana_program::pubkey::Pubkey::new_from_array([#((#no_update_tracking_owners),)*]),)*
        ];

        pub const CHAIN_ID_LIST: [(u64, &str, ::solana_program::pubkey::Pubkey); #chains_len] = [
            #( (#chain_ids, #chain_names, ::solana_program::pubkey::Pubkey::new_from_array([#(#chain_tokens),*])) ),*
        ];
    }
    .into()
}

#[proc_macro]
pub fn common_config_parser(tokens: TokenStream) -> TokenStream {
    let config = parse_macro_input!(tokens as CommonConfig);

    let mut variables = BTreeMap::new();
    let mut tokens = Vec::<proc_macro2::TokenStream>::new();

    for v in config.variables {
        let t = v.r#type;
        let name = v.name;
        let value = v.value;

        let elf_name_string = "NEON_".to_string() + &name.to_string();
        let elf_name = Ident::new(&elf_name_string, name.span());
        let elf_value = match &value {
            syn::Lit::Str(s) => s.clone(),
            syn::Lit::Int(i) => LitStr::new(&i.to_string(), i.span()),
            syn::Lit::Float(f) => LitStr::new(&f.to_string(), f.span()),
            syn::Lit::Bool(b) => LitStr::new(&b.value().to_string(), b.span()),
            _ => unreachable!(),
        };

        tokens.push(quote! {
            pub const #name: #t = #value;
            neon_elf_param!(#elf_name, #elf_value);
        });

        variables.insert(elf_name_string, elf_value);
    }

    let variables_len = variables.len();
    let variable_names = variables.keys();
    let variable_values = variables.values();

    quote! {
        #(#tokens)*

        pub const PARAMETERS: [(&str, &str); #variables_len] = [
            #( (#variable_names, #variable_values) ),*
        ];
    }
    .into()
}

#[proc_macro_derive(ReconstructRaw)]
pub fn reconstruct_raw(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = parse_macro_input!(input as DeriveInput);

    let Struct(DataStruct {
        fields: Named(FieldsNamed { ref named, .. }),
        ..
    }) = ast.data
    else {
        unimplemented!("ReconstructRaw only works for structs");
    };
    let builder_fields = named.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;

        // If the type of the field is Vector, use a special function to reconstruct it.
        // Only Vectors of primitive types are supported.
        // Other Vectors (including Vector<Vector<T>>) are constructed empty.
        // N.B. Currently, it's only used in the Transaction in the context of the Core API.
        // The only composite vector is access_list which is not relevant for the Core API.
        if !is_vector_type(ty) {
            quote! { #name: std::ptr::read_unaligned(std::ptr::addr_of!((*struct_ptr).#name)) }
        } else if is_composite_vector_type(ty) {
            quote! { #name: vector![] }
        } else {
            quote! { #name: read_vec(std::ptr::addr_of!((*struct_ptr).#name).cast::<usize>(), offset).into_vector() }
        }
    });

    let name = &ast.ident;
    quote! {
        impl ReconstructRaw for #name {
            /// # Safety
            /// Generated code, if something goes wrong here, it likely won't compile at all.
            unsafe fn build(struct_ptr: *const Self, offset: isize) -> Self {
                unsafe {
                    Self {
                        #(#builder_fields,)*
                    }
                }
            }
        }
    }
    .into()
}

fn is_vector_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath {
            path: path_type, ..
        }) => path_type
            .segments
            .iter()
            .any(|f| f.ident.to_string().eq("Vector")),
        Type::Tuple(TypeTuple {
            elems: elems_type, ..
        }) => elems_type.iter().any(is_vector_type),
        _ => false,
    }
}

fn is_argument_vector_type(arg: &PathArguments) -> bool {
    match arg {
        PathArguments::AngleBracketed(inner_arg) => inner_arg.args.iter().any(|f| match f {
            GenericArgument::Type(inner_type) => is_vector_type(inner_type),
            _ => false,
        }),
        _ => false,
    }
}

fn is_composite_vector_type(ty: &Type) -> bool {
    if let Type::Path(TypePath {
        qself: _,
        path: path_type,
    }) = ty
    {
        let vec_path_segment = path_type
            .segments
            .iter()
            .find(|&f| f.ident.to_string().eq("Vector"));
        if let Some(path_segment) = vec_path_segment {
            return is_argument_vector_type(&path_segment.arguments);
        }
        return false;
    }
    false
}
