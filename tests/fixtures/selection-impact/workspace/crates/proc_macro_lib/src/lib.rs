extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn make_greeting(_item: TokenStream) -> TokenStream {
    "\"Hello from proc macro!\"".parse().unwrap()
}
