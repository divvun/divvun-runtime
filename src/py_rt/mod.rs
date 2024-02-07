// py.run

use pyo3::types::PyDict;
use tempfile::tempdir;

use crate::ast::PipelineDefinition;

pub fn interpret_pipeline(input: &str) -> PipelineDefinition {
    use pyo3::prelude::*;
    // use pyo3::types::IntoPyDict;

    let tmp = tempdir().unwrap();
    crate::py::generate(tmp.path().join("divvun_runtime")).unwrap();

    let py_res: PyResult<Option<PipelineDefinition>> = Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let locals = PyDict::new(py);
        let globals = PyDict::new(py);
        locals.set_item("sys", sys)?;

        py.eval(
            &format!("sys.path.append({:?})", tmp.path().display()),
            Some(globals),
            Some(locals),
        )?;
        let path: Vec<String> = sys.getattr("path").unwrap().extract()?;

        let pipeline_mod = PyModule::from_code(py, input, "pipeline.py", "pipeline")?;
        let json_mod = PyModule::from_code(py, TO_JSON, "to_json.py", "to_json")?;

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
            let res = json_mod
                .getattr("to_json")?
                .call1((res,))?
                .extract::<String>()
                .unwrap();
            let pd: PipelineDefinition = serde_json::from_str(&res).unwrap();
            return Ok(Some(pd));
        } else {
            println!("NO CALL");
        }

        Ok(None)
    });

    py_res.unwrap().unwrap()
}

const TO_JSON: &str = r#"
from divvun_runtime import *
import json
from json import JSONEncoder

def to_json(obj: Command) -> str:
    class Encoder(JSONEncoder):
        def default(self, o: Any):
            return o.__dict__

    return json.dumps({"ast": obj}, cls=Encoder)
"#;
