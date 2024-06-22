use crate::{_init_py, PYTHON};

pub fn repl() -> i32 {
    _init_py().py_runmain()
}
