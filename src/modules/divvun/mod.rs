mod blanktag;
mod cgspell;
mod suggest;

pub use blanktag::Blanktag;
pub use cgspell::Cgspell;
pub use suggest::Suggest;

use crate::modules::{Arg, Command, Module, Ty};

inventory::submit! {
    Module {
        name: "divvun",
        commands: &[
            Command {
                name: "blanktag",
                args: &[Arg { name: "model_path", ty: Ty::Path }],
                init: Blanktag::new,
            },
            Command {
                name: "cgspell",
                args: &[
                    Arg {name: "err_model_path", ty: Ty::Path },
                    Arg {name: "acc_model_path", ty: Ty::Path },
                ],
                init: Cgspell::new,
            },
            Command {
                name: "suggest",
                args: &[
                    Arg {name: "model_path", ty: Ty::Path },
                    Arg {name: "error_xml_path", ty: Ty::Path },
                ],
                init: Suggest::new,
            }
        ]
    }
}
