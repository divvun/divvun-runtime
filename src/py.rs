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

    std::fs::write(output_path.join("__init__.py"), INIT_PY)?;

    for module in inventory::iter::<Module> {
        let py_fn = output_path.join(module.name).with_extension("py");
        std::fs::write(
            &py_fn,
            generate_py(module)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "format failed"))?,
        )?;
    }

    std::fs::write(output_path.join("py.typed"), "")?;

    Ok(())
}

fn generate_py(module: &Module) -> Result<String, std::fmt::Error> {
    let mut s = String::from(PY_HEADER);

    for command in module.commands {
        write!(&mut s, "def {}(input: Input", command.name)?;
        if !command.args.is_empty() {
            write!(&mut s, ", *")?;
        }
        for arg in command.args {
            write!(&mut s, ", {}: {}", arg.name, arg.ty.as_py_type())?;
        }
        writeln!(&mut s, ") -> Command:")?;
        writeln!(&mut s, "    return Command(")?;
        writeln!(&mut s, "        module=\"{}\",", module.name)?;
        writeln!(&mut s, "        command=\"{}\",", command.name)?;
        writeln!(&mut s, "        input=input,")?;
        writeln!(
            &mut s,
            "        returns=\"{}\",",
            command.returns.as_dr_type()
        )?;
        if !command.args.is_empty() {
            writeln!(&mut s, "        args={{")?;
            for arg in command.args {
                writeln!(
                    &mut s,
                    "            \"{}\": Arg(type=\"{}\", value={}),",
                    arg.name,
                    arg.ty.as_dr_type(),
                    arg.name
                )?;
            }
            writeln!(&mut s, "        }}")?;
        }
        writeln!(&mut s, "    )\n")?;
    }

    Ok(s)
}

const PY_HEADER: &str = r#"from . import Arg, Command, Input
from typing import *

"#;

const INIT_PY: &str = include_str!("./init.py");

#[test]
fn lol() {
    generate("./lolpy").unwrap();
}
