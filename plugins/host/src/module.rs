use anyhow::{Context, Result, ensure};
use libloading::Library;
use serde::Serialize;
use std::{
    ffi::{c_char, c_void},
    path::Path,
};
use vst3::{ComPtr, Interface, Steinberg::*};

#[derive(Clone, Serialize)]
pub struct PluginClass {
    pub id: String,
    pub name: String,
    pub category: String,
}

pub struct Module {
    factory: Option<ComPtr<IPluginFactory>>,
    library: Library,
}
impl Module {
    pub fn open(path: &Path) -> Result<Self> {
        // All third-party code runs in this expendable worker process.
        unsafe {
            let library = Library::new(path).context("The VST3 binary could not be loaded")?;
            #[cfg(windows)]
            if let Ok(init) = library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") {
                ensure!(init(), "VST3 module initialization failed");
            }
            #[cfg(unix)]
            {
                let native: libloading::os::unix::Library = library.into();
                let handle = native.into_raw();
                let library: Library = libloading::os::unix::Library::from_raw(handle).into();
                if let Ok(init) =
                    library.get::<unsafe extern "system" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
                {
                    ensure!(init(handle), "VST3 module entry failed");
                }
                return Self::factory(library);
            }
            #[cfg(not(unix))]
            Self::factory(library)
        }
    }
    unsafe fn factory(library: Library) -> Result<Self> {
        let factory = unsafe {
            let get = library
                .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0")
                .context("Not a VST3 module: GetPluginFactory is missing")?;
            ComPtr::from_raw(get()).context("Plugin returned a null factory")?
        };
        Ok(Self {
            factory: Some(factory),
            library,
        })
    }
    pub fn classes(&self) -> Result<Vec<PluginClass>> {
        let factory = self.factory.as_ref().unwrap();
        let count = unsafe { factory.countClasses() };
        ensure!(
            (0..=1024).contains(&count),
            "Plugin returned an invalid class count"
        );
        let mut output = Vec::new();
        for i in 0..count {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            ensure!(
                unsafe { factory.getClassInfo(i, &mut info) } == kResultOk,
                "Cannot read VST3 class {i}"
            );
            let category = text(&info.category);
            if category == "Audio Module Class" {
                output.push(PluginClass {
                    id: hex(&info.cid),
                    name: text(&info.name),
                    category,
                });
            }
        }
        ensure!(
            !output.is_empty(),
            "Module contains no audio processor classes"
        );
        Ok(output)
    }
    pub unsafe fn create<I: Interface>(&self, cid: &TUID) -> Result<ComPtr<I>> {
        let mut object: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            self.factory.as_ref().unwrap().createInstance(
                cid.as_ptr().cast(),
                I::IID.as_ptr().cast(),
                &mut object,
            )
        };
        ensure!(
            result == kResultOk,
            "Cannot create requested VST3 interface ({result})"
        );
        unsafe { ComPtr::from_raw(object.cast()).context("Plugin returned a null interface") }
    }
}
impl Drop for Module {
    fn drop(&mut self) {
        drop(self.factory.take());
        unsafe {
            #[cfg(windows)]
            if let Ok(exit) = self
                .library
                .get::<unsafe extern "system" fn() -> bool>(b"ExitDll\0")
            {
                exit();
            }
            #[cfg(unix)]
            if let Ok(exit) = self
                .library
                .get::<unsafe extern "system" fn() -> bool>(b"ModuleExit\0")
            {
                exit();
            }
        }
    }
}
fn text(bytes: &[c_char]) -> String {
    String::from_utf8_lossy(
        &bytes
            .iter()
            .map(|&c| c as u8)
            .take_while(|&c| c != 0)
            .collect::<Vec<_>>(),
    )
    .into_owned()
}
fn hex(id: &TUID) -> String {
    id.iter().map(|&b| format!("{:02x}", b as u8)).collect()
}
pub fn parse_id(text: &str) -> Result<TUID> {
    ensure!(
        text.len() == 32 && text.is_ascii(),
        "VST3 class ID must contain 32 hex digits"
    );
    let mut id = [0; 16];
    for (i, slot) in id.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)? as _;
    }
    Ok(id)
}
