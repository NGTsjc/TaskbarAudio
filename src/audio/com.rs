use std::marker::PhantomData;
use std::rc::Rc;
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};

// Both apartment lifetime and allocated buffers stay on the owning thread.
pub(super) struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    pub fn new() -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub(super) struct ComAllocation<T>(pub *mut T);

impl<T> Drop for ComAllocation<T> {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast())) };
    }
}
