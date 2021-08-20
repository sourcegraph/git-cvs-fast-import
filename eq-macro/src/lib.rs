use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(EqU8)]
pub fn eq_u8_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl PartialEq<[u8]> for #name {
            fn eq(&self, other: &[u8]) -> bool {
                self.0.eq(other)
            }
        }
    };

    TokenStream::from(expanded)
}
