use std::{
    ffi::{c_char, CStr, CString},
    fs::File,
    io::Write,
    path::Path,
    sync::Arc,
};

use ast::{from_ast, PipelineDefinition};
use modules::Context;
use tempfile::TempDir;

pub mod ast;
pub mod modules;

pub async fn run() -> anyhow::Result<()> {
    Ok(())
}

pub struct Bundle {
    temp_dir: TempDir,
    defn: PipelineDefinition,
}

impl Bundle {
    pub fn load<P: AsRef<Path>>(bundle_path: P) -> anyhow::Result<Bundle> {
        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        box_file.extract_all(temp_dir.path())?;

        let txt = std::fs::read_to_string(temp_dir.path().join("ast.json"))?;
        let jd = &mut serde_json::Deserializer::from_str(&txt);
        let defn: PipelineDefinition = serde_path_to_error::deserialize(jd)?;

        Ok(Bundle { temp_dir, defn })
    }

    pub async fn run_pipeline(
        &self,
        context: Arc<Context>,
        input: String,
    ) -> anyhow::Result<String> {
        let result = from_ast(
            context,
            self.defn.ast.clone(),
            Box::pin(async { Ok(input) }),
        )?
        .await?;
        Ok(result)
    }

    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }
}

#[no_mangle]
extern "C" fn bundle_load(path: *const c_char) -> *mut Bundle {
    let bytes = unsafe { CStr::from_ptr(path).to_bytes() };
    let path = std::str::from_utf8(bytes).unwrap();

    let bundle = Bundle::load(Path::new(path)).unwrap();
    std::env::set_current_dir(bundle.path()).unwrap();

    Box::into_raw(Box::new(bundle))
}

#[no_mangle]
extern "C" fn bundle_run_pipeline(bundle: *mut Bundle, input: *const c_char) -> *mut c_char {
    let bytes = unsafe { CStr::from_ptr(input).to_bytes() };
    let input = std::str::from_utf8(bytes).unwrap();

    let context: Context = Context {
        path: unsafe { bundle.as_ref().unwrap() }.path().to_path_buf(),
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let res = rt.handle().block_on(
        unsafe { bundle.as_ref().unwrap() }.run_pipeline(Arc::new(context), input.to_string()),
    );

    // let Ok(res) = res else {
    //     // Return empty string
    //     return CString::new("nope").unwrap().into_raw();
    // };

    let res = match res {
        Ok(result) => result,
        Err(error) => {
            return CString::new(format!("Error: {}", error))
                .unwrap()
                .into_raw()
        }
    };

    CString::new(res).unwrap().into_raw()
}
