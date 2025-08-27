use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use once_cell::sync::Lazy;

use crate::modules::{CommandDef, Module};

pub static MODULES: once_cell::sync::Lazy<HashMap<String, HashMap<String, CommandDef>>> =
    Lazy::new(|| {
        let mut m = HashMap::new();

        for module in inventory::iter::<Module>() {
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

    for module in inventory::iter::<Module> {
        let ts_fn = output_path.join(module.name).with_extension("ts");

        std::fs::write(
            &ts_fn,
            generate_ts(module)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "format failed"))?,
        )?;
    }

    Ok(())
}

fn generate_ts(module: &Module) -> Result<String, std::fmt::Error> {
    let mut s = String::from(TS_HEADER);

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
                writeln!(&mut s, "    {}: {};", arg.name, arg.ty.as_ts_type())?;
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
        writeln!(
            &mut s,
            "        returns: \"{}\",",
            command.returns.as_dr_type()
        )?;
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
}

impl AsTypeScriptType for crate::modules::Ty {
    fn as_ts_type(&self) -> String {
        let mut out = vec![];
        if self.contains(crate::modules::Ty::Path) {
            out.push("string");
        }
        if self.contains(crate::modules::Ty::String) {
            out.push("string");
        }
        if self.contains(crate::modules::Ty::Json) {
            out.push("any");
        }
        if self.contains(crate::modules::Ty::Bytes) {
            out.push("Uint8Array");
        }
        if self.contains(crate::modules::Ty::Int) {
            out.push("number");
        }
        if self.contains(crate::modules::Ty::ArrayString) {
            out.push("string[]");
        }
        if self.contains(crate::modules::Ty::ArrayBytes) {
            out.push("Uint8Array[]");
        }
        if self.contains(crate::modules::Ty::MapPath) {
            out.push("Record<string, string>");
        }
        if self.contains(crate::modules::Ty::MapString) {
            out.push("Record<string, string>");
        }
        if self.contains(crate::modules::Ty::MapBytes) {
            out.push("Record<string, Uint8Array>");
        }
        out.join(" | ")
    }
}

#[test]
fn lol() {
    generate("./lol_ts").unwrap();
}
