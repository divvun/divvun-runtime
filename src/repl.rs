use crate::{init_py, PYTHON};

pub fn repl() -> i32 {
    init_py();
    unsafe { PYTHON.take().unwrap().py_runmain() }
}
