//! Small COM host objects. Calls use SDK-owned buffers only inside the worker.
#![allow(unsafe_op_in_unsafe_fn)]
use std::ffi::c_void;
use vst3::{
    Class, ComPtr, ComWrapper,
    Steinberg::{Vst::*, *},
};

pub struct Host;
impl Class for Host {
    type Interfaces = (IHostApplication, IComponentHandler);
}
impl IHostApplicationTrait for Host {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        *name = [0; 128];
        for (slot, ch) in (*name)
            .iter_mut()
            .zip("SlimeyBeats-EZ-Master".encode_utf16())
        {
            *slot = ch as _;
        }
        kResultOk
    }
    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if !obj.is_null() {
            *obj = std::ptr::null_mut();
        }
        kNoInterface
    }
}
impl IComponentHandlerTrait for Host {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn performEdit(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }
    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn restartComponent(&self, _flags: i32) -> tresult {
        kResultOk
    }
}

pub struct Point {
    id: ParamID,
    value: ParamValue,
}
impl Class for Point {
    type Interfaces = (IParamValueQueue,);
}
impl IParamValueQueueTrait for Point {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id
    }
    unsafe fn getPointCount(&self) -> i32 {
        1
    }
    unsafe fn getPoint(&self, index: i32, offset: *mut i32, value: *mut ParamValue) -> tresult {
        if index != 0 || offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        *offset = 0;
        *value = self.value;
        kResultOk
    }
    unsafe fn addPoint(&self, _offset: i32, _value: ParamValue, _index: *mut i32) -> tresult {
        kNotImplemented
    }
}
pub struct Changes {
    points: Vec<ComPtr<IParamValueQueue>>,
}
impl Changes {
    pub fn new(values: &[(u32, f64)]) -> Self {
        Self {
            points: values
                .iter()
                .map(|&(id, value)| {
                    ComWrapper::new(Point { id, value })
                        .to_com_ptr::<IParamValueQueue>()
                        .unwrap()
                })
                .collect(),
        }
    }
}
impl Class for Changes {
    type Interfaces = (IParameterChanges,);
}
impl IParameterChangesTrait for Changes {
    unsafe fn getParameterCount(&self) -> i32 {
        self.points.len() as i32
    }
    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        self.points
            .get(index as usize)
            .map_or(std::ptr::null_mut(), |p| p.as_ptr())
    }
    unsafe fn addParameterData(
        &self,
        _id: *const ParamID,
        _index: *mut i32,
    ) -> *mut IParamValueQueue {
        std::ptr::null_mut()
    }
}
