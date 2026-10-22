use std::marker::PhantomData;
use std::rc::Rc;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetWindowDpiAwarenessContext, SetThreadDpiAwarenessContext,
};

/// Keep Win32 geometry calls in the parent window's coordinate space. A child
/// created with a different awareness can force a DPI reset on SetParent.
pub struct DpiScope {
    previous: DPI_AWARENESS_CONTEXT,
    _thread_bound: PhantomData<Rc<()>>,
}

impl DpiScope {
    pub fn for_window(hwnd: HWND) -> anyhow::Result<Self> {
        let context = unsafe { GetWindowDpiAwarenessContext(hwnd) };
        Self::enter(context)
    }

    pub fn per_monitor() -> anyhow::Result<Self> {
        Self::enter(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    }

    fn enter(context: DPI_AWARENESS_CONTEXT) -> anyhow::Result<Self> {
        anyhow::ensure!(!context.0.is_null(), "invalid window DPI context");
        let previous = unsafe { SetThreadDpiAwarenessContext(context) };
        anyhow::ensure!(!previous.0.is_null(), "cannot set thread DPI context");
        Ok(Self {
            previous,
            _thread_bound: PhantomData,
        })
    }
}

impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::HiDpi::{AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext};

    #[test]
    fn context_is_restored_after_scope() {
        let previous = unsafe { GetThreadDpiAwarenessContext() };
        {
            let _scope = DpiScope::per_monitor().unwrap();
            assert!(
                unsafe {
                    AreDpiAwarenessContextsEqual(
                        GetThreadDpiAwarenessContext(),
                        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                    )
                }
                .as_bool()
            );
        }
        assert!(
            unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), previous,) }
                .as_bool()
        );
    }
}
