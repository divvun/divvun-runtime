use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use once_cell::sync::Lazy;

use crate::modules::{CommandDef, Module};

pub static MODULES: once_cell::sync::Lazy<HashMap<String, HashMap<String, CommandDef>>> =
    Lazy::new(|| {
        let mut m = HashMap::new();

        for module in crate::modules::get_modules().iter() {
            m.insert(
                module.name.to_string(),
                module
                    .commands
                    .iter()
                    .map(|command| (command.name.to_string(), command.clone()))
                    .collect::<HashMap<_, _>>(),
            );
        }
        m
    });

pub fn generate<P: AsRef<Path>>(output_path: P) -> std::io::Result<()> {
    let output_path = output_path.as_ref();
    std::fs::create_dir_all(output_path)?;

    // Generate main index.ts file
    std::fs::write(output_path.join("mod.ts"), INDEX_TS)?;

    for module in crate::modules::get_modules().iter() {
        let ts_fn = output_path.join(module.name).with_extension("ts");

        std::fs::write(
            &ts_fn,
            generate_ts(&module)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "format failed"))?,
        )?;
    }

    Ok(())
}

fn generate_ts(module: &Module) -> Result<String, std::fmt::Error> {
    let mut s = String::from(TS_HEADER);

    // Generate struct interfaces for this module
    for struct_def in crate::modules::get_structs() {
        if struct_def.module == module.name {
            writeln!(&mut s, "export interface {} {{", struct_def.name)?;
            for field in struct_def.fields {
                let optional_marker = if field.optional { "?" } else { "" };
                writeln!(
                    &mut s,
                    "    {}{}: {};",
                    field.name, optional_marker, field.ty
                )?;
            }
            writeln!(&mut s, "}}\n")?;
        }
    }

    for command in module.commands {
        // Generate options interface if there are arguments
        if !command.args.is_empty() {
            writeln!(
                &mut s,
                "interface {}Options {{",
                command
                    .name
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .collect::<String>()
                    + &command.name[1..]
            )?;
            for arg in command.args {
                let optional_marker = if arg.optional { "?" } else { "" };
                writeln!(
                    &mut s,
                    "    {}{}: {};",
                    arg.name,
                    optional_marker,
                    arg.ty.as_ts_type()
                )?;
            }
            writeln!(&mut s, "}}\n")?;
        }

        // Generate overload signatures
        if command.args.is_empty() {
            // No options needed
            writeln!(
                &mut s,
                "export function {}(id: string, input: Input): Command;",
                command.name
            )?;
            writeln!(
                &mut s,
                "export function {}(input: Input): Command;",
                command.name
            )?;
        } else {
            // With options object
            let options_type = format!(
                "{}Options",
                command
                    .name
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .collect::<String>()
                    + &command.name[1..]
            );
            writeln!(
                &mut s,
                "export function {}(id: string, input: Input, options: {}): Command;",
                command.name, options_type
            )?;
            writeln!(
                &mut s,
                "export function {}(input: Input, options: {}): Command;",
                command.name, options_type
            )?;
        }

        // Generate implementation
        if command.args.is_empty() {
            // Simple case: no options
            writeln!(
                &mut s,
                "export function {}(arg1: string | Input, arg2?: Input): Command {{",
                command.name
            )?;
            writeln!(&mut s, "    const hasId = typeof arg1 === 'string';")?;
            writeln!(&mut s, "    const id = hasId ? arg1 : undefined;")?;
            writeln!(&mut s, "    const input = hasId ? arg2! : arg1 as Input;")?;
        } else {
            // With options object
            let options_type = format!(
                "{}Options",
                command
                    .name
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .collect::<String>()
                    + &command.name[1..]
            );
            writeln!(
                &mut s,
                "export function {}(arg1: string | Input, arg2: Input | {}, arg3?: {}): Command {{",
                command.name, options_type, options_type
            )?;
            writeln!(&mut s, "    const hasId = typeof arg1 === 'string';")?;
            writeln!(&mut s, "    const id = hasId ? arg1 : undefined;")?;
            writeln!(
                &mut s,
                "    const input = hasId ? arg2 as Input : arg1 as Input;"
            )?;
            writeln!(
                &mut s,
                "    const options = hasId ? arg3! : arg2 as {};",
                options_type
            )?;
        }

        writeln!(&mut s, "    return new Command({{")?;
        writeln!(&mut s, "        id,")?;
        writeln!(&mut s, "        module: \"{}\",", module.name)?;
        writeln!(&mut s, "        command: \"{}\",", command.name)?;
        writeln!(&mut s, "        input,")?;

        // Use schema-aware type string for returns field
        let returns_type = command.returns.as_ts_type_with_schema(command.schema);
        writeln!(
            &mut s,
            "        returns: \"{}\",",
            command.returns.as_dr_type()
        )?;

        if let Some(schema) = command.schema {
            writeln!(&mut s, "        schema: \"{}\",", schema)?;
        }
        if let Some(kind) = command.kind {
            writeln!(&mut s, "        kind: \"{}\",", kind)?;
        }
        if !command.args.is_empty() {
            writeln!(&mut s, "        args: {{")?;
            for arg in command.args {
                writeln!(
                    &mut s,
                    "            {}: new Arg(\"{}\", options.{}),",
                    arg.name,
                    arg.ty.as_dr_type(),
                    arg.name
                )?;
            }
            writeln!(&mut s, "        }}")?;
        }
        writeln!(&mut s, "    }});")?;
        writeln!(&mut s, "}}\n")?;
    }

    Ok(s)
}

const TS_HEADER: &str = r#"import { Arg, Command, Input } from './mod.ts';

"#;

const INDEX_TS: &str = include_str!("./init.ts");

// Extension trait to convert Rust types to TypeScript types
trait AsTypeScriptType {
    fn as_ts_type(&self) -> String;
    fn as_ts_type_with_schema(&self, schema: Option<&str>) -> String;
}

impl AsTypeScriptType for crate::modules::Ty {
    fn as_ts_type(&self) -> String {
        self.as_ts_type_with_schema(None)
    }

    fn as_ts_type_with_schema(&self, schema: Option<&str>) -> String {
        use crate::modules::Ty;
        match self {
            Ty::Path => "string".to_string(),
            Ty::String => "string".to_string(),
            Ty::Json => {
                // Use schema if provided, otherwise default to 'any'
                schema
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "any".to_string())
            }
            Ty::Bytes => "Uint8Array".to_string(),
            Ty::Int => "number".to_string(),
            Ty::ArrayString => "string[]".to_string(),
            Ty::ArrayBytes => "Uint8Array[]".to_string(),
            Ty::MapPath => "Record<string, string>".to_string(),
            Ty::MapString => "Record<string, string>".to_string(),
            Ty::MapBytes => "Record<string, Uint8Array>".to_string(),
            Ty::Struct(name) => name.to_string(),
            Ty::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| t.as_ts_type()).collect();
                type_strs.join(" | ")
            }
        }
    }
}

#[test]
fn lol() {
    generate("./lol_ts").unwrap();
}
