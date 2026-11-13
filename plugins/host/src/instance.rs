use crate::{host_context::Host, module::Module};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use vst3::{
    ComPtr, ComWrapper,
    Steinberg::{Vst::*, *},
};

#[derive(Clone, Serialize)]
pub struct Parameter {
    pub id: u32,
    pub name: String,
    pub units: String,
    pub normalized: f64,
    pub default_normalized: f64,
    pub read_only: bool,
    pub steps: i32,
}

pub struct Instance {
    pub component: ComPtr<IComponent>,
    pub processor: ComPtr<IAudioProcessor>,
    pub controller: Option<ComPtr<IEditController>>,
    separate_controller: bool,
    _host: ComPtr<IHostApplication>,
    _handler: ComPtr<IComponentHandler>,
}
impl Instance {
    pub fn new(module: &Module, cid: &TUID) -> Result<Self> {
        unsafe {
            let host = ComWrapper::new(Host);
            let host_application = host.to_com_ptr::<IHostApplication>().unwrap();
            let handler = host.to_com_ptr::<IComponentHandler>().unwrap();
            let component = module.create::<IComponent>(cid)?;
            ensure!(
                component.initialize(host_application.as_ptr().cast()) == kResultOk,
                "Plugin component initialize failed"
            );
            let processor = component
                .cast::<IAudioProcessor>()
                .context("Class is not an audio processor")?;
            let mut separate_controller = false;
            let controller = if let Some(controller) = component.cast::<IEditController>() {
                Some(controller)
            } else {
                let mut id = [0; 16];
                if component.getControllerClassId(&mut id) == kResultOk {
                    let controller = module.create::<IEditController>(&id)?;
                    ensure!(
                        controller.initialize(host_application.as_ptr().cast()) == kResultOk,
                        "Plugin controller initialize failed"
                    );
                    separate_controller = true;
                    Some(controller)
                } else {
                    None
                }
            };
            if let Some(controller) = &controller {
                controller.setComponentHandler(handler.as_ptr());
            }
            if separate_controller
                && let (Some(a), Some(b)) = (
                    component.cast::<IConnectionPoint>(),
                    controller
                        .as_ref()
                        .and_then(|c| c.cast::<IConnectionPoint>()),
                )
            {
                a.connect(b.as_ptr());
                b.connect(a.as_ptr());
            }
            Ok(Self {
                component,
                processor,
                controller,
                separate_controller,
                _host: host_application,
                _handler: handler,
            })
        }
    }
    pub fn parameters(&self) -> Result<Vec<Parameter>> {
        let Some(controller) = &self.controller else {
            return Ok(vec![]);
        };
        let count = unsafe { controller.getParameterCount() };
        ensure!(
            (0..=16384).contains(&count),
            "Invalid plugin parameter count"
        );
        let mut parameters = Vec::new();
        for i in 0..count {
            let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
            ensure!(
                unsafe { controller.getParameterInfo(i, &mut info) } == kResultOk,
                "Cannot read plugin parameter"
            );
            parameters.push(Parameter {
                id: info.id,
                name: wide(&info.title),
                units: wide(&info.units),
                normalized: unsafe { controller.getParamNormalized(info.id) },
                default_normalized: info.defaultNormalizedValue,
                read_only: info.flags & 2 != 0,
                steps: info.stepCount,
            });
        }
        Ok(parameters)
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.processor.setProcessing(0);
            self.component.setActive(0);
            if self.separate_controller
                && let Some(controller) = &self.controller
            {
                if let (Some(a), Some(b)) = (
                    self.component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                ) {
                    a.disconnect(b.as_ptr());
                    b.disconnect(a.as_ptr());
                }
                controller.setComponentHandler(std::ptr::null_mut());
                controller.terminate();
            }
            self.component.terminate();
        }
    }
}
pub fn wide(text: &[TChar]) -> String {
    String::from_utf16_lossy(
        &text
            .iter()
            .copied()
            .take_while(|&c| c != 0)
            .collect::<Vec<_>>(),
    )
}
