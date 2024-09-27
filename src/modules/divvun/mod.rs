mod blanktag;
mod cgspell;
mod normalize;
mod suggest;

pub use blanktag::Blanktag;
pub use cgspell::Cgspell;
pub use suggest::Suggest;
// pub use normalize::Normalize;

use crate::modules::{Arg, Command, Module, Ty};

inventory::submit! {
    Module {
        name: "divvun",
        commands: &[
            Command {
                name: "blanktag",
                input: &[Ty::String],
                args: &[Arg { name: "model_path", ty: Ty::Path }],
                init: Blanktag::new,
                returns: Ty::String,
            },
            Command {
                name: "cgspell",
                input: &[Ty::String],
                args: &[
                    Arg {name: "err_model_path", ty: Ty::Path },
                    Arg {name: "acc_model_path", ty: Ty::Path },
                ],
                init: Cgspell::new,
                returns: Ty::String,
            },
            Command {
                name: "suggest",
                input: &[Ty::String],
                args: &[
                    Arg {name: "model_path", ty: Ty::Path },
                    Arg {name: "error_xml_path", ty: Ty::Path },
                ],
                init: Suggest::new,
                returns: Ty::Json,
            },
            // Command {
            //     name: "normalize",
            //     input: &[Ty::String],
            //     args: &[],
            //     init: Normalize::new,
            //     returns: Ty::String,
            // }
        ]
    }
}
