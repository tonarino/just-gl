use crate::drm::DrmDisplay;
use drm::control::{framebuffer::Handle as FramebufferHandle, Device as ControlDevice};
use gbm::{BufferObject, BufferObjectFlags, Format as BufferFormat, Surface};

pub struct Window {
    pub(crate) gbm_surface: Surface<FramebufferHandle>,
    pub(crate) drm_display: DrmDisplay,
    pub(crate) frame_count: usize,
    previous_bo: Option<BufferObject<FramebufferHandle>>,
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

        Window { gbm_surface, drm_display, frame_count: 0, previous_bo: None }
    }

    /// # Safety
    /// this must be called exactly once after `eglSwapBuffers`,
    /// which happens e.g. in `Frame::finish()`.
    unsafe fn present(&mut self) {
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

        if self.frame_count == 0 {
            // NOTE(mbernat): It's possible to avoid initial mode setting
            // since we're keeping the previous mode: we could just call page_flip directly.
            self.drm_display.set_mode_with_framebuffer(Some(fb));
        } else {
            self.drm_display.page_flip(fb);
        }

        // NOTE(mbernat): This buffer object returns to the surface's queue upon destruction.
        // If we were to release it here, it would be again available from
        // `Surface::lock_front_buffer()` next and the app would be effectively single-buffered.
        // So, we keep it around for one frame, which is fine for double buffering.
        self.previous_bo.replace(buffer_object);

        self.frame_count += 1;
    }

    pub fn restore_original_display(&self) {
        let handle = self.drm_display.crtc.framebuffer().unwrap();
        if self.drm_display.gbm_device.get_framebuffer(handle).is_ok() {
            self.drm_display.set_mode_with_framebuffer(self.drm_display.crtc.framebuffer());
        }
    }

    pub fn draw(&mut self, mut drawer: impl FnMut()) {
        drawer();
        // SAFETY: eglSwapBuffers is called by `frame.finish()` in drawer()
        unsafe { self.present() };

        // The first page flip is scheduled after frame #1 (which is the second frame)
        // Yes, this is very stupid, just testing if it works

        if self.frame_count > 1 {
            let _events =
                self.drm_display.gbm_device.receive_events().expect("Could not receive events");

            // TODO(mbernat): We could do additional processing
            // on these events if they report page flips that we scheduled
            // but with the current setup there is nothing we need to do.
        }
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
