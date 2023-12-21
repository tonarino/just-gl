use crate::drm::DrmDisplay;
use drm::control::{framebuffer::Handle as FramebufferHandle, Device as ControlDevice, Event};
use gbm::{BufferObjectFlags, Format as BufferFormat, Surface};

pub struct Window {
    pub(crate) gbm_surface: Surface<FramebufferHandle>,
    pub(crate) drm_display: DrmDisplay,
    pub(crate) crtc_set: bool,
}

impl Window {
    pub fn new(drm_display: DrmDisplay) -> Window {
        let format = BufferFormat::Argb8888;
        // NOTE(mbernat): nvidia driver does not implement `create_surface`
        // and presumably one should use this variant instead. But the program crashes later
        // anyway when creating buffers with "Invalid argument" (22) kernel error.
        /*
        let modifiers = std::iter::once(Modifier::Linear);
        let gbm_surface: Surface<FramebufferHandle> = drm_display
            .gbm_device
            .create_surface_with_modifiers(drm_display.width, drm_display.height, format, modifiers)
            .unwrap();
        */

        let usage = BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING;
        let gbm_surface: Surface<FramebufferHandle> = drm_display
            .gbm_device
            .create_surface(drm_display.width, drm_display.height, format, usage)
            .unwrap();

        Window { gbm_surface, drm_display, crtc_set: false }
    }

    // TODO(mbernat): Add a "Frame" abstraction that calls `swap_buffers` internally when
    // it's finished (just like glium's Frame does) so that users don't need to bother with this.

    /// # Safety
    /// this must be called exactly once after `eglSwapBuffers`,
    /// which happens e.g. in `Frame::finish()`.
    pub unsafe fn swap_buffers(&mut self) {
        // TODO(mbernat): move this elsewhere
        let depth_bits = 24;
        let bits_per_pixel = 32;

        // SAFETY: we offloaded the `lock_front_buffer()` precondition to our caller
        let mut buffer_object = unsafe { self.gbm_surface.lock_front_buffer().unwrap() };

        // NOTE(mbernat): Frame buffer recycling:
        // we store an FB handle in buffer object's user_data() and reuse the FB when it exists
        let data = buffer_object.userdata().expect("Could not get buffer object user data");
        let fb = if let Some(handle) = data {
            *handle
        } else {
            let fb = self
                .drm_display
                .gbm_device
                .add_framebuffer(&buffer_object, depth_bits, bits_per_pixel)
                .unwrap();
            buffer_object.set_userdata(fb).expect("Could not set buffer object user data");
            fb
        };

        if !self.crtc_set {
            self.crtc_set = true;
            self.drm_display.set_mode_with_framebuffer(Some(fb));
        } else {
            self.drm_display.page_flip(fb);
        }
    }

    pub fn restore_original_display(&self) {
        self.drm_display.set_mode_with_framebuffer(self.drm_display.crtc.framebuffer());
    }

    pub fn draw(&mut self, drawer: impl Fn()) {
        if self.crtc_set {
            let mut events =
                self.drm_display.gbm_device.receive_events().expect("Could not receive events");

            for event in events {
                match event {
                    Event::PageFlip(drm::control::PageFlipEvent { frame, duration, crtc }) => {
                        println!("PageFlip {frame} {duration:?} {crtc:?}");
                    },
                    Event::Vblank(event) => {
                        println!("Vblank");
                    },
                    _ => {},
                }
            }
        }

        drawer();
        // SAFETY: eglSwapBuffers is called by `frame.finish()`
        unsafe { self.swap_buffers() };

        //std::thread::sleep(std::time::Duration::from_secs_f64(0.02));
    }
}

mod rwh_impl {
    /* SAFETY NOTICE
    Safety of these implementations is not enforced statically, it just happens to be the case
    right now because we control everything. If we were providing this code as a library the user
    could easily drop the display or window and then try rendering to them.

    To make this safer, one should tie together window's and handle's lifetimes.
    I believe raw-window-handle 0.6 does that by providing safe versions of these traits [1], [2].
    Unfortunately, glutin 0.30 uses rwh version 0.5.

    [1] https://docs.rs/raw-window-handle/0.6.0/raw_window_handle/trait.HasDisplayHandle.html
    [2] https://docs.rs/raw-window-handle/0.6.0/raw_window_handle/trait.HasWindowHandle.html
    */

    use super::Window;
    use gbm::AsRaw;
    use raw_window_handle::*;

    // SAFETY: surface is valid for the duration of the program
    unsafe impl HasRawWindowHandle for Window {
        fn raw_window_handle(&self) -> RawWindowHandle {
            let mut handle = GbmWindowHandle::empty();
            handle.gbm_surface = self.gbm_surface.as_raw() as *mut _;
            RawWindowHandle::Gbm(handle)
        }
    }

    // SAFETY: device is valid for the duration of the program
    unsafe impl HasRawDisplayHandle for Window {
        fn raw_display_handle(&self) -> RawDisplayHandle {
            let mut handle = GbmDisplayHandle::empty();
            handle.gbm_device = self.drm_display.gbm_device.as_raw() as *mut _;
            RawDisplayHandle::Gbm(handle)
        }
    }
}
