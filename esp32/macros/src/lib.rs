//! `#[esp32::main]`, re-exported by `esp32`. See there.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, ItemFn};

/// Turn `async fn main() { .. }` into a plain `fn main` that runs the body with
/// `esp32::block_on`.
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut main = parse_macro_input!(item as ItemFn);
    if !args.is_empty() {
        let args = proc_macro2::TokenStream::from(args);
        return syn::Error::new_spanned(args, "#[esp32::main] takes no arguments")
            .to_compile_error()
            .into();
    }
    if main.sig.asyncness.take().is_none() {
        return syn::Error::new_spanned(main.sig.fn_token, "#[esp32::main] needs an `async fn`")
            .to_compile_error()
            .into();
    }
    let body = &main.block;
    main.block = parse_quote!({ ::esp32::block_on(async move #body) });
    quote!(#main).into()
}
