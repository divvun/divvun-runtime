use std::sync::Arc;

use once_cell::sync::Lazy;
use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::types::{PyDict, PyTuple};
use tempfile::tempdir;

use crate::ast::PipelineDefinition;

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

    println!("Get temp dir");
    let tmp = tempdir().unwrap();
    println!("Generate");
    match crate::py::generate(tmp.path().join("divvun_runtime")) {
        Ok(v) => {},
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("OH NO");
        }
    }

    println!("Uh??");

    let py_res: PyResult<Option<serde_json::Value>> = Python::with_gil(|py| {
        println!("Add to path in py");
        let sys = py.import("sys")?;
        let locals = PyDict::new(py);
        let globals = PyDict::new(py);
        locals.set_item("sys", sys)?;

        py.eval(
            &format!("sys.path.append({:?})", tmp.path().display()),
            Some(globals),
            Some(locals),
        )?;

        println!("Load pipeline and divvun runtime");
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

        println!("Run callback");
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
    println!("Interpret pipeline inner");
    let res = dump_ast(input)?;
    println!("Get json");
    let pd: PipelineDefinition = serde_json::from_value(res).unwrap();
    Ok(pd)
}
