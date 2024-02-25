use crate::{init_py, INIT};

pub fn repl() -> i32 {
    init_py();
    unsafe { INIT.take().unwrap().py_runmain() }
}
