use divvun_runtime::modules;

fn main() {
    println!("=== Registered Modules and Commands ===\n");

    for module in modules::get_modules() {
        println!("Module: {}", module.name);
        for command in module.commands {
            println!("  Command: {}", command.name);

            // Format input types
            let input_types: Vec<_> = command.input.iter().map(|ty| ty.as_dr_type()).collect();
            println!("    Input types: [{}]", input_types.join(", "));

            println!("    Returns: {}", command.returns.as_dr_type());

            if !command.args.is_empty() {
                println!("    Arguments:");
                for arg in command.args {
                    let optional_marker = if arg.optional { "?" } else { "" };
                    println!(
                        "      - {}{}: {}",
                        arg.name,
                        optional_marker,
                        arg.ty.as_dr_type()
                    );
                }
            }
        }
        println!();
    }

    println!("=== Registered Struct Definitions ===\n");

    let structs: Vec<_> = modules::get_structs().collect();
    if structs.is_empty() {
        println!("No struct definitions registered yet.");
    } else {
        for struct_def in structs {
            println!("Struct: {}::{}", struct_def.module, struct_def.name);
            println!("  Fields:");
            for field in struct_def.fields {
                let optional_marker = if field.optional { "?" } else { "" };
                println!("    - {}{}: {}", field.name, optional_marker, field.ty);
            }
            println!();
        }
    }
}
