use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use once_cell::sync::Lazy;

use crate::modules::{Command, Module};

pub static MODULES: once_cell::sync::Lazy<HashMap<String, HashMap<String, Command>>> =
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

pub fn generate<P: AsRef<Path>>(output_path: P) -> anyhow::Result<()> {
    let output_path = output_path.as_ref();
    std::fs::create_dir_all(output_path)?;

    std::fs::write(output_path.join("__init__.py"), INIT_PY)?;

    for module in inventory::iter::<Module> {
        let py_fn = output_path.join(module.name).with_extension("py");
        std::fs::write(&py_fn, generate_py(module)?)?;
    }

    std::fs::write(output_path.join("py.typed"), "")?;

    Ok(())
}

fn generate_py(module: &Module) -> anyhow::Result<String> {
    let mut s = String::from(PY_HEADER);

    for command in module.commands {
        write!(&mut s, "def {}(input: Input", command.name)?;
        for arg in command.args {
            write!(&mut s, ", {}: {}", arg.name, arg.ty.as_py_type())?;
        }
        writeln!(&mut s, ") -> Command:")?;
        writeln!(&mut s, "    return Command(")?;
        writeln!(&mut s, "        module=\"{}\",", module.name)?;
        writeln!(&mut s, "        command=\"{}\",", command.name)?;
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
            writeln!(&mut s, "        }},")?;
        }
        writeln!(&mut s, "        input=input")?;
        writeln!(&mut s, "    )\n")?;
    }

    Ok(s)
}

const PY_HEADER: &str = r#"from . import Arg, Command, Input

"#;

const INIT_PY: &str = r#"from typing import Any, Dict, Optional, Union, Literal, Callable


ValueType = Literal['string', 'path']

class _Entry:
    def __init__(self, value_type: ValueType):
        self.type = "entry"
        self.value_type = value_type


class StringEntry(_Entry):
    def __init__(self):
        super().__init__("string")


class PathEntry(_Entry):
    def __init__(self):
        super().__init__("path")


class Arg:
    def __init__(self, type: str, value: Optional[str]):
        self.type = type
        self.value = value

Input = Union["Command", _Entry]

class Command:
    def __init__(
        self,
        module: str,
        command: str,
        args: Optional[Dict[str, Arg]] = None,
        input: Optional[Input] = None,
    ):
        self.type = "command"
        self.module = module
        self.command = command
        if args is not None:
            self.args = args
        self.input = input

def pipeline(func: Callable[..., Any]) -> Callable[..., Any]:
    entry = func.__annotations__.get("entry", None)
    if entry is None:
        raise ValueError(f"Pipeline function missing `entry` argument")
    if not issubclass(entry, _Entry):
        raise ValueError(f"Pipeline function `entry` argument must be an Entry subclass")

    def wrapper():
        return func(entry())
    setattr(wrapper, "_is_pipeline", True)
    return wrapper
"#;

#[test]
fn lol() {
    generate("./lolpy").unwrap();
}
