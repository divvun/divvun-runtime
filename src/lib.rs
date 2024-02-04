use std::{
    ffi::{c_char, CStr, CString},
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use ast::{from_ast, PipelineDefinition};
use log::LevelFilter;
use modules::{Context, Input};
use oslog::OsLogger;
use tempfile::TempDir;

pub mod ast;
pub mod modules;
pub mod py;
pub mod py_rt;

pub async fn run() -> anyhow::Result<()> {
    Ok(())
}

pub enum BundleContentsPath {
    TempDir(TempDir),
    Literal(PathBuf),
}

impl BundleContentsPath {
    pub fn path(&self) -> &Path {
        match self {
            BundleContentsPath::TempDir(p) => p.path(),
            BundleContentsPath::Literal(p) => p,
        }
    }
}

pub struct Bundle {
    temp_dir: BundleContentsPath,
    defn: PipelineDefinition,
}

impl Bundle {
    pub fn load<P: AsRef<Path>>(bundle_path: P) -> anyhow::Result<Bundle> {
        // For writing to a file when debugging as a dynamic library
        // let f = File::create("/tmp/divvun_runtime.log").unwrap();
        // tracing_subscriber::fmt()
        //     .with_writer(f)
        //     .without_time()
        //     .init();

        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        box_file.extract_all(temp_dir.path())?;

        let txt = std::fs::read_to_string(temp_dir.path().join("ast.json"))?;
        let jd = &mut serde_json::Deserializer::from_str(&txt);
        let defn: PipelineDefinition = serde_path_to_error::deserialize(jd)?;

        Ok(Bundle {
            temp_dir: BundleContentsPath::TempDir(temp_dir),
            defn,
        })
    }

    pub fn new<P: AsRef<Path>>(defn: PipelineDefinition, contents_path: P) -> Bundle {
        Bundle {
            temp_dir: BundleContentsPath::Literal(contents_path.as_ref().to_path_buf()),
            defn,
        }
    }

    pub async fn run_pipeline(&self, context: Arc<Context>, input: Input) -> anyhow::Result<Input> {
        tracing::info!("Running pipeline");
        std::env::set_var("PATH", "/usr/local/bin:/opt/divvun/bin");

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
    OsLogger::new("nu.necessary.Divvun")
        .level_filter(LevelFilter::Debug)
        .init()
        .unwrap();

    let bytes = unsafe { CStr::from_ptr(path).to_bytes() };
    let path = std::str::from_utf8(bytes).unwrap();

    let bundle = Bundle::load(Path::new(path)).unwrap();
    // std::env::set_current_dir(bundle.path()).unwrap();

    tracing::info!("Load bundle: {:?}", bundle.path());

    Box::into_raw(Box::new(bundle))
}

#[no_mangle]
extern "C" fn bundle_run_pipeline(bundle: *mut Bundle, input: *const c_char) -> *mut c_char {
    let bytes = unsafe { CStr::from_ptr(input).to_bytes() };
    let input = std::str::from_utf8(bytes).unwrap();

    let context: Context = Context {
        path: unsafe { bundle.as_ref().unwrap() }.path().to_path_buf(),
    };

    tracing::info!("Run pipeline: {:?}", context.path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let res = rt.handle().block_on(
        unsafe { bundle.as_ref().unwrap() }
            .run_pipeline(Arc::new(context), input.to_string().into()),
    );

    // let Ok(res) = res else {
    //     // Return empty string
    //     return CString::new("nope").unwrap().into_raw();
    // };

    let res = match res {
        Ok(result) => result.try_into_string().unwrap(),
        Err(error) => {
            return CString::new(format!("Error: {}", error))
                .unwrap()
                .into_raw()
        }
    };

    CString::new(res).unwrap().into_raw()
}

#[no_mangle]
extern "C" fn bundle_run_pipeline_bytes(bundle: *mut Bundle, input: FfiString) -> FfiSlice {
    log::error!("MCPLS HI");
    let input = unsafe { input.as_str() }.to_string();
    let bundle = unsafe { bundle.as_ref().unwrap() };
    log::error!("MCPLS HI 2 {:?}", input);

    let context: Context = Context {
        path: bundle.path().to_path_buf(),
    };
    log::error!("MCPLS HI 3 {:?}", &context);

    let rt = tokio::runtime::Runtime::new().unwrap();
    log::error!("MCPLS HI 4");

    std::panic::set_hook(Box::new(|err| {
        if let Some(x) = err.payload().downcast_ref::<&str>() {
            log::error!("MCPLS HI 5 {:?}", &x);
        }
        if let Some(x) = err.payload().downcast_ref::<String>() {
            log::error!("MCPLS HI 5 {:?}", &x);
        }
        log::error!("PANIC");
    }));

    let res = rt
        .handle()
        .block_on(bundle.run_pipeline(Arc::new(context), input.into()));

    log::error!("MCPLS HI 5 {:?}", &res);
    let res = match res {
        Ok(result) => result.try_into_bytes().unwrap(),
        Err(_error) => {
            return FfiSlice {
                data: std::ptr::null_mut(),
                len: 0,
            };
        }
    };

    // Box::into_raw(res.into_boxed_slice()) as *mut _
    FfiSlice::from(res)
}

#[repr(C)]
#[derive(Debug)]
pub struct FfiString {
    data: *mut u8,
    len: usize,
}

impl FfiString {
    unsafe fn as_str(&self) -> &str {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data as *const u8, self.len))
    }
}

impl From<String> for FfiString {
    fn from(data: String) -> FfiString {
        FfiString {
            len: data.len(),
            data: Box::into_raw(data.into_boxed_str()) as *mut _,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct FfiSlice {
    data: *mut u8,
    len: usize,
}

impl FfiSlice {
    unsafe fn as_bytes(&self) -> &[u8] {
        std::slice::from_raw_parts(self.data as *const u8, self.len)
    }
}

impl From<Vec<u8>> for FfiSlice {
    fn from(data: Vec<u8>) -> FfiSlice {
        FfiSlice {
            len: data.len(),
            data: Box::into_raw(data.into_boxed_slice()) as *mut _,
        }
    }
}
