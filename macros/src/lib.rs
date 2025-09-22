use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, ItemImpl, Type};
use unsynn::*;

/// Proc macro for registering command implementations
///
/// Usage:
/// ```rust
/// #[rt_command(
///     module = "divvun",
///     name = "blanktag",
///     input = [String],
///     output = "String",
///     args(model_path = "Path")
/// )]
/// impl Blanktag {
///     // implementation...
/// }
/// ```
#[proc_macro_attribute]
pub fn rt_command(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_impl = parse_macro_input!(input as ItemImpl);

    match expand_divvun_command(args, input_impl) {
        Ok(tokens) => tokens.into(),
        Err(err) => {
            let err_msg = format!("{}", err);
            quote! { compile_error!(#err_msg); }.into()
        }
    }
}

fn expand_divvun_command(
    args: TokenStream,
    input_impl: ItemImpl,
) -> std::result::Result<TokenStream2, Box<dyn std::error::Error>> {
    // Convert proc_macro::TokenStream to proc_macro2::TokenStream
    let args2: TokenStream2 = args.into();
    // Parse attributes using unsynn
    let mut token_iter = args2.to_token_iter();
    let attrs = parse_command_attributes(&mut token_iter)?;

    // Get the type being implemented
    let impl_type = match &*input_impl.self_ty {
        Type::Path(type_path) => &type_path.path,
        _ => return Err("Expected path type".into()),
    };

    // Generate the static command definition
    let module = &attrs.module;
    let name = &attrs.name;
    let command_def_name = format!(
        "__{}_{}__COMMAND_DEF",
        module.to_uppercase().replace('-', "_"),
        name.to_uppercase().replace('-', "_")
    );
    let command_def_ident =
        proc_macro2::Ident::new(&command_def_name, proc_macro2::Span::call_site());

    // Convert input types to Ty enum variants
    let mut input_ty_tokens = Vec::new();
    for ty in &attrs.input {
        let token = match ty.as_str() {
            "String" => quote! { crate::modules::Ty::String },
            "Bytes" => quote! { crate::modules::Ty::Bytes },
            "Json" => quote! { crate::modules::Ty::Json },
            "Path" => quote! { crate::modules::Ty::Path },
            "Int" => quote! { crate::modules::Ty::Int },
            "ArrayString" => quote! { crate::modules::Ty::ArrayString },
            "ArrayBytes" => quote! { crate::modules::Ty::ArrayBytes },
            "MapPath" => quote! { crate::modules::Ty::MapPath },
            "MapString" => quote! { crate::modules::Ty::MapString },
            "MapBytes" => quote! { crate::modules::Ty::MapBytes },
            _ => return Err(format!("Unknown input type: {}", ty).into()),
        };
        input_ty_tokens.push(token);
    }

    // Convert output type
    let output_ty_token = match attrs.output.as_str() {
        "String" => quote! { crate::modules::Ty::String },
        "Bytes" => quote! { crate::modules::Ty::Bytes },
        "Json" => quote! { crate::modules::Ty::Json },
        "Path" => quote! { crate::modules::Ty::Path },
        "Int" => quote! { crate::modules::Ty::Int },
        "ArrayString" => quote! { crate::modules::Ty::ArrayString },
        "ArrayBytes" => quote! { crate::modules::Ty::ArrayBytes },
        "MapPath" => quote! { crate::modules::Ty::MapPath },
        "MapString" => quote! { crate::modules::Ty::MapString },
        "MapBytes" => quote! { crate::modules::Ty::MapBytes },
        _ => return Err(format!("Unknown output type: {}", attrs.output).into()),
    };

    // Convert argument definitions
    let mut args_tokens = Vec::new();
    for (arg_name, arg_type) in &attrs.args {
        let arg_type_token = match arg_type.as_str() {
            "String" => quote! { crate::modules::Ty::String },
            "Bytes" => quote! { crate::modules::Ty::Bytes },
            "Json" => quote! { crate::modules::Ty::Json },
            "Path" => quote! { crate::modules::Ty::Path },
            "Int" => quote! { crate::modules::Ty::Int },
            "ArrayString" => quote! { crate::modules::Ty::ArrayString },
            "ArrayBytes" => quote! { crate::modules::Ty::ArrayBytes },
            "MapPath" => quote! { crate::modules::Ty::MapPath },
            "MapString" => quote! { crate::modules::Ty::MapString },
            "MapBytes" => quote! { crate::modules::Ty::MapBytes },
            _ => {
                return Err(format!(
                    "Unknown argument type '{}' for argument '{}'",
                    arg_type, arg_name
                )
                .into())
            }
        };

        args_tokens.push(quote! {
            crate::modules::Arg {
                name: #arg_name,
                ty: #arg_type_token,
            }
        });
    }

    // Convert asset dependencies
    let assets_tokens: Vec<TokenStream2> = attrs
        .assets
        .iter()
        .map(|asset| match asset {
            AssetDepDef::Required(path) => quote! {
                crate::modules::AssetDep::Required(#path)
            },
            AssetDepDef::RequiredRegex(pattern) => quote! {
                crate::modules::AssetDep::RequiredRegex(#pattern)
            },
            AssetDepDef::Optional(path) => quote! {
                crate::modules::AssetDep::Optional(#path)
            },
            AssetDepDef::OptionalRegex(pattern) => quote! {
                crate::modules::AssetDep::OptionalRegex(#pattern)
            },
        })
        .collect();

    let expanded = quote! {
        #input_impl

        // Generate static command definition
        #[allow(non_upper_case_globals)]
        static #command_def_ident: crate::modules::CommandDef = crate::modules::CommandDef {
            name: #name,
            module: #module,
            input: &[#(#input_ty_tokens),*],
            args: &[#(#args_tokens),*],
            assets: &[#(#assets_tokens),*],
            init: #impl_type::new,
            returns: #output_ty_token,
        };

        // Submit the command definition to inventory
        inventory::submit! {
            &#command_def_ident
        }
    };

    Ok(expanded)
}

// Simple attribute structure
#[derive(Debug)]
struct CommandAttrs {
    module: String,
    name: String,
    input: Vec<String>,
    output: String,
    args: Vec<(String, String)>,
    assets: Vec<AssetDepDef>,
}

#[derive(Debug)]
enum AssetDepDef {
    Required(String),
    RequiredRegex(String),
    Optional(String),
    OptionalRegex(String),
}

fn parse_command_attributes(token_iter: &mut TokenIter) -> unsynn::Result<CommandAttrs> {
    let mut module = None;
    let mut name = None;
    let mut input = None;
    let mut output = None;
    let mut args = Vec::new();
    let mut assets = Vec::new();

    // Parse comma-separated attribute items
    loop {
        // Try to parse an identifier - if it fails, we're done
        let ident: Ident = match token_iter.parse() {
            Ok(ident) => ident,
            Err(_) => break,
        };

        let _eq: Operator<'='> = token_iter.parse()?;

        match ident.to_string().as_str() {
            "module" => {
                let lit: LiteralString = token_iter.parse()?;
                module = Some(lit.as_str().to_string());
            }
            "name" => {
                let lit: LiteralString = token_iter.parse()?;
                name = Some(lit.as_str().to_string());
            }
            "input" => {
                let group: BracketGroupContaining<CommaDelimitedVec<Ident>> = token_iter.parse()?;
                let types: Vec<String> = group
                    .content
                    .0
                    .iter()
                    .map(|delimited_item| delimited_item.value.to_string())
                    .collect();
                input = Some(types);
            }
            "output" => {
                let lit: LiteralString = token_iter.parse()?;
                output = Some(lit.as_str().to_string());
            }
            "args" => {
                // For args, we expect brackets containing comma-delimited arg definitions
                let group: BracketGroupContaining<CommaDelimitedVec<ArgDefPair>> =
                    token_iter.parse()?;
                for delimited_item in &group.content.0 {
                    let arg_def = &delimited_item.value;
                    args.push((arg_def.name.to_string(), arg_def.ty.as_str().to_string()));
                }
            }
            "assets" => {
                // For assets, we expect brackets containing comma-delimited function calls
                let group: BracketGroupContaining<CommaDelimitedVec<AssetFuncCall>> =
                    token_iter.parse()?;
                for delimited_item in &group.content.0 {
                    let asset_call = &delimited_item.value;
                    let func_name = asset_call.func_name.to_string();
                    let arg_str = asset_call.arg.as_str();

                    match func_name.as_str() {
                        "required" => {
                            // Check if this has an 'r' prefix (r"...")
                            if asset_call.r_prefix.is_some() {
                                // It's a regex pattern
                                assets.push(AssetDepDef::RequiredRegex(arg_str.to_string()));
                            } else {
                                // It's a literal path
                                assets.push(AssetDepDef::Required(arg_str.to_string()));
                            }
                        }
                        "optional" => {
                            // Check if this has an 'r' prefix (r"...")
                            if asset_call.r_prefix.is_some() {
                                // It's a regex pattern
                                assets.push(AssetDepDef::OptionalRegex(arg_str.to_string()));
                            } else {
                                // It's a literal path
                                assets.push(AssetDepDef::Optional(arg_str.to_string()));
                            }
                        }
                        _ => {
                            return Error::other(
                                token_iter,
                                format!("Unknown asset function: {}", func_name),
                            )
                        }
                    }
                }
            }
            _ => return Error::other(token_iter, "Unknown attribute".to_string()),
        }

        // Try to parse comma separator
        if token_iter.parse::<Operator<','>>().is_ok() {
            continue;
        } else {
            break;
        }
    }

    let module = match module {
        Some(m) => m,
        None => return Error::other(token_iter, "module attribute required".to_string()),
    };
    let name = match name {
        Some(n) => n,
        None => return Error::other(token_iter, "name attribute required".to_string()),
    };
    let input = match input {
        Some(i) => i,
        None => return Error::other(token_iter, "input attribute required".to_string()),
    };
    let output = match output {
        Some(o) => o,
        None => return Error::other(token_iter, "output attribute required".to_string()),
    };

    Ok(CommandAttrs {
        module,
        name,
        input,
        output,
        args,
        assets,
    })
}

// Define custom parser for arg definitions
unsynn! {
    struct ArgDefPair {
        name: Ident,
        eq: Operator<'='>,
        ty: LiteralString,
    }
}

// Define custom parser for asset function calls like required("file") or optional(r"pattern")
unsynn! {
    struct AssetFuncCall {
        func_name: Ident,
        open_paren: Operator<'('>,
        r_prefix: Option<Operator<'r'>>,
        arg: LiteralString,
        close_paren: Operator<')'>,
    }
}
