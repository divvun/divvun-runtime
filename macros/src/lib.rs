use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Field, Fields, ItemImpl, ItemStruct, Type};
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
    for (arg_name, arg_type, is_optional) in &attrs.args {
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
            custom_type => {
                // For custom struct types, use Struct variant with the type name
                quote! { crate::modules::Ty::Struct(#custom_type) }
            }
        };

        args_tokens.push(quote! {
            crate::modules::Arg {
                name: #arg_name,
                ty: #arg_type_token,
                optional: #is_optional,
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

    // Generate kind token
    let kind_token = if let Some(ref kind_str) = attrs.kind {
        quote! { Some(#kind_str) }
    } else {
        quote! { None }
    };

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
            kind: #kind_token,
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
    args: Vec<(String, String, bool)>, // name, type, optional
    assets: Vec<AssetDepDef>,
    kind: Option<String>,
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
    let mut kind = None;

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
            "kind" => {
                let lit: LiteralString = token_iter.parse()?;
                kind = Some(lit.as_str().to_string());
            }
            "args" => {
                // For args, we expect brackets containing comma-delimited arg definitions
                let group: BracketGroupContaining<CommaDelimitedVec<ArgDefPair>> =
                    token_iter.parse()?;
                for delimited_item in &group.content.0 {
                    let arg_def = &delimited_item.value;
                    let is_optional = arg_def.optional.is_some();
                    args.push((
                        arg_def.name.to_string(),
                        arg_def.ty.as_str().to_string(),
                        is_optional,
                    ));
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
        kind,
    })
}

// Define custom parser for arg definitions
unsynn! {
    struct ArgDefPair {
        name: Ident,
        optional: Option<Operator<'?'>>,
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

/// Proc macro for registering struct definitions for TypeScript generation
///
/// Usage:
/// ```rust
/// #[rt_struct(module = "divvun")]
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// pub struct MyConfig {
///     #[serde(default)]
///     pub optional_field: Option<usize>,
///     pub required_field: String,
/// }
/// ```
#[proc_macro_attribute]
pub fn rt_struct(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_struct = parse_macro_input!(input as ItemStruct);

    match expand_rt_struct(args, input_struct) {
        Ok(tokens) => tokens.into(),
        Err(err) => {
            let err_msg = format!("{}", err);
            quote! { compile_error!(#err_msg); }.into()
        }
    }
}

fn expand_rt_struct(
    args: TokenStream,
    input_struct: ItemStruct,
) -> std::result::Result<TokenStream2, Box<dyn std::error::Error>> {
    let struct_name = &input_struct.ident;
    let struct_name_str = struct_name.to_string();

    // Parse the module parameter
    let args2: TokenStream2 = args.into();
    let mut token_iter = args2.to_token_iter();
    let module_name = parse_struct_module(&mut token_iter)?;

    // Extract field information
    let fields = match &input_struct.fields {
        Fields::Named(fields_named) => &fields_named.named,
        _ => return Err("rt_struct only supports structs with named fields".into()),
    };

    let mut field_definitions = Vec::new();

    for field in fields {
        let field_name = field
            .ident
            .as_ref()
            .ok_or("Field must have a name")?
            .to_string();

        // Check if field is optional by looking for Option<T> in the type and extract TypeScript type
        let (type_str, is_optional) = extract_field_type_info(&field)?;

        field_definitions.push(quote! {
            crate::modules::StructField {
                name: #field_name,
                ty: #type_str,
                optional: #is_optional,
            }
        });
    }

    // Generate the static struct definition
    let struct_def_name = format!("__{}_STRUCT_DEF", struct_name_str.to_uppercase());
    let struct_def_ident =
        proc_macro2::Ident::new(&struct_def_name, proc_macro2::Span::call_site());

    let expanded = quote! {
        #input_struct

        // Generate static struct definition
        #[allow(non_upper_case_globals)]
        static #struct_def_ident: crate::modules::StructDef = crate::modules::StructDef {
            name: #struct_name_str,
            module: #module_name,
            fields: &[#(#field_definitions),*],
        };

        // Submit the struct definition to inventory
        inventory::submit! {
            &#struct_def_ident
        }
    };

    Ok(expanded)
}

fn extract_field_type_info(
    field: &Field,
) -> std::result::Result<(String, bool), Box<dyn std::error::Error>> {
    use syn::{GenericArgument, Type};

    // Check if the field has a #[serde(default)] attribute to help determine optionality
    let has_serde_default = field.attrs.iter().any(|attr| {
        if let Ok(meta) = attr.meta.require_list() {
            if meta.path.is_ident("serde") {
                return meta.tokens.to_string().contains("default");
            }
        }
        false
    });

    // Extract type information
    let (ts_type, is_option) = match &field.ty {
        Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Option" {
                    // It's Option<T>, extract T
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner_type)) = args.args.first() {
                            let inner_str = quote!(#inner_type).to_string();
                            (map_rust_type_to_ts(&inner_str), true)
                        } else {
                            ("any".to_string(), true)
                        }
                    } else {
                        ("any".to_string(), true)
                    }
                } else {
                    // Regular type
                    let type_str = quote!(#type_path).to_string();
                    (map_rust_type_to_ts(&type_str), false)
                }
            } else {
                ("any".to_string(), false)
            }
        }
        _ => {
            // Other types (references, arrays, etc.)
            let type_str = quote!(#field.ty).to_string();
            (map_rust_type_to_ts(&type_str), false)
        }
    };

    // Field is optional if it's Option<T> OR has #[serde(default)]
    let is_optional = is_option || has_serde_default;

    Ok((ts_type, is_optional))
}

fn map_rust_type_to_ts(rust_type: &str) -> String {
    match rust_type.trim() {
        "String" | "&str" | "str" => "string".to_string(),
        "f32" | "f64" | "i32" | "i64" | "u32" | "u64" | "usize" | "isize" => "number".to_string(),
        "bool" => "boolean".to_string(),
        ty if ty.starts_with("Vec <") || ty.starts_with("Vec<") => {
            // Extract inner type from Vec<T>
            if let Some(start) = ty.find('<') {
                if let Some(end) = ty.rfind('>') {
                    let inner = &ty[start + 1..end].trim();
                    format!("{}[]", map_rust_type_to_ts(inner))
                } else {
                    "any[]".to_string()
                }
            } else {
                "any[]".to_string()
            }
        }
        // For custom types, assume they're interfaces that will be generated
        _ => rust_type.to_string(),
    }
}

fn parse_struct_module(token_iter: &mut TokenIter) -> unsynn::Result<String> {
    // Parse: module = "divvun"
    let ident: Ident = token_iter.parse()?;
    if ident.to_string() != "module" {
        return Error::other(token_iter, "Expected 'module' parameter".to_string());
    }

    let _eq: Operator<'='> = token_iter.parse()?;
    let lit: LiteralString = token_iter.parse()?;

    Ok(lit.as_str().to_string())
}
