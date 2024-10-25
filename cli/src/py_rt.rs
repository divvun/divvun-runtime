use std::{cell::RefCell, sync::Arc};

use once_cell::sync::Lazy;
use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::types::{PyDict, PyTuple};
use tempfile::tempdir;

use divvun_runtime::ast::PipelineDefinition;

thread_local! {
    pub static PYTHON: RefCell<Option<MainPythonInterpreter<'static, 'static>>> = RefCell::new(None);
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Python(#[from] pyo3::PyErr),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub fn dump_ast(input: &str) -> Result<serde_json::Value, Error> {
    use pyo3::prelude::*;

    let tmp = tempdir().unwrap();
    match divvun_runtime::py::generate(tmp.path().join("divvun_runtime")) {
        Ok(v) => {}
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("OH NO");
        }
    }

    let py_res: PyResult<Option<serde_json::Value>> = Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let locals = PyDict::new(py);
        let globals = PyDict::new(py);
        locals.set_item("sys", sys)?;

        py.eval(
            &format!("sys.path.append({:?})", tmp.path().display()),
            Some(globals),
            Some(locals),
        )?;

        let pipeline_mod = PyModule::from_code(py, input, "pipeline.py", "pipeline")?;
        let divvun_runtime_mod = py.import("divvun_runtime")?;

        let callback: Option<PyObject> = pipeline_mod
            .dict()
            .iter()
            .filter_map(|(_k, v)| {
                if v.hasattr("_is_pipeline").unwrap() {
                    Some(v.into_py(py))
                } else {
                    None
                }
            })
            .next();

        if let Some(callback) = callback {
            let res = callback.call0(py)?;
            let res = res.downcast::<PyTuple>(py)?;
            let res = divvun_runtime_mod
                .getattr("_to_json")?
                .call(res, None)?
                .extract::<String>()
                .unwrap();
            return Ok(Some(serde_json::from_str(&res).unwrap()));
        } else {
            println!("NO CALL");
        }

        Ok(None)
    });

    match py_res {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(Error::Python(pyo3::PyErr::new::<
            pyo3::exceptions::PyTypeError,
            _,
        >("No pipeline found"))),
        Err(e) => Err(Error::Python(e)),
    }
}

pub fn interpret_pipeline(input: &str) -> Result<PipelineDefinition, Error> {
    let res = dump_ast(input)?;
    let pd: PipelineDefinition = serde_json::from_value(res).unwrap();
    Ok(pd)
}

pub(crate) fn _init_py() -> MainPythonInterpreter<'static, 'static> {
    // let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
    const ARTIFACT_PATH: Option<&str> = option_env!("ARTIFACT_PATH");
    let pythonhome = std::env::var_os("PYTHONHOME").or_else(|| ARTIFACT_PATH.map(Into::into));
    // log.error(&format!("PY INIT TIME: {pythonhome:?}"));

    use pathos::AppDirs;
    let app_dirs = pathos::user::AppDirs::new("Divvun Runtime").unwrap();
    let cache_path = app_dirs.cache_dir().join("py");
    let _ = std::fs::create_dir_all(&cache_path);

    // log.error(&format!("Cache path: {}", cache_path.display()));
    // unsafe {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.interpreter_config.isolated = Some(true);
    config.interpreter_config.home = pythonhome.map(Into::into);
    config.argv = Some(vec![]);
    // log.error(&format!("{:#?}", &config));
    let interp = match MainPythonInterpreter::new(config) {
        Ok(interp) => interp,
        Err(e) => {
            // log.error(&format!("{e}"));
            panic!("{}", e);
        }
    };

    // if let Ok(virtual_env) = std::env::var("VIRTUAL_ENV") {
    //     interp.with_gil(|py| {
    //         let syspath: &PyList = py
    //             .import("sys")
    //             .unwrap()
    //             .getattr("path")
    //             .unwrap()
    //             .downcast()
    //             .unwrap();
    //         syspath
    //             .append(format!("{}/lib/python3.11/site-packages", virtual_env).into_py(py))
    //             .unwrap();
    //     });
    // }

    interp
    // }
}

#[inline(always)]
pub fn init_py() {
    PYTHON.with_borrow_mut(|py| {
        if py.is_none() {
            *py = Some(_init_py());
        } else {
            // Do nothing
        }
    })
}

pub fn repl() -> i32 {
    _init_py().py_runmain()
}
