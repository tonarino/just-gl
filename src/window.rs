use crate::drm::DrmDisplay;
use drm::control::{framebuffer::Handle as FramebufferHandle, Device as ControlDevice};
use gbm::{BufferObject, BufferObjectFlags, Format as BufferFormat, Surface};

enum DisplayState {
    Init,
    // NOTE(mbernat): Buffer objects obtained from a surface
    // return to the surface's queue when dropped, so we need to store them here.
    //
    // Unfortunately, this behavior is not very well documented,
    // see `Surface::lock_front_buffer()` implementation for details.
    ModeSet { _buffer_object: BufferObject<FramebufferHandle> },
    PageFlipScheduled { _buffer_object: BufferObject<FramebufferHandle> },
}

pub struct Window {
    pub(crate) gbm_surface: Surface<FramebufferHandle>,
    pub(crate) drm_display: DrmDisplay,
    display_state: DisplayState,
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
            .expect("Could not create a GBM surface");
        */

        let usage = BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING;
        let gbm_surface: Surface<FramebufferHandle> = drm_display
            .gbm_device
            .create_surface(drm_display.width, drm_display.height, format, usage)
            .expect("Could not create a GBM surface");

        Window { gbm_surface, drm_display, display_state: DisplayState::Init }
    }

    /// # Safety
    /// The buffer we are trying to present must be valid.
    /// Defining validity precisely needs more work but it likely involves
    /// writing to the buffer so that it does not contain uninitialized memory.
    ///
    /// One way to achieve this is with `glium`'s `Frame::finish()`,
    /// which calls `eglSwapBuffers()` internally and that in turns calls
    /// `glFlush()` to write to the buffer.
    pub unsafe fn present(&mut self) {
        // TODO(mbernat): move this elsewhere
        let depth_bits = 24;
        let bits_per_pixel = 32;

        // SAFETY: we offloaded the `lock_front_buffer()` precondition to our caller
        let mut buffer_object = unsafe { self.gbm_surface.lock_front_buffer() }
            .expect("Could not obtain a buffer object from the GBM surface");

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
                .expect("Could not add a frame buffer");
            buffer_object.set_userdata(fb).expect("Could not set buffer object user data");
            fb
        };

        self.display_state = match &self.display_state {
            DisplayState::Init => {
                // NOTE(mbernat): Displays often use their preferred modes.
                // If that's already the case we can avoid initial mode setting and go
                // straight to page flipping as a small optimization.
                self.drm_display.set_mode_with_framebuffer(fb);
                DisplayState::ModeSet { _buffer_object: buffer_object }
            },
            DisplayState::ModeSet { .. } | DisplayState::PageFlipScheduled { .. } => {
                self.drm_display.schedule_page_flip(fb);
                // The buffer object we store here will hang around for a frame
                // and will be dropped by this match arm in the next frame;
                // this is sufficient for double buffering.
                DisplayState::PageFlipScheduled { _buffer_object: buffer_object }
            },
        };

        // Page flips are scheduled asynchronously, so we need to await their completion.
        if matches!(self.display_state, DisplayState::PageFlipScheduled { .. }) {
            // This call is blocking and should not be called when no events are expected.
            // Its implementation is just a read from the DRM file descriptor, which should
            // be replaced by e.g. an `epoll` over multiple sources in a proper event loop.
            let _events =
                self.drm_display.gbm_device.receive_events().expect("Could not receive events");

            // TODO(mbernat): We could do additional processing
            // on these events if they report page flips that we scheduled
            // but with the current setup there is nothing we need to do.
        }
    }

    pub fn restore_original_display(&self) {
        let handle = self
            .drm_display
            .crtc
            .framebuffer()
            .expect("Window should have a CRTC framebuffer handle");
        if let Ok(fb) = self.drm_display.gbm_device.get_framebuffer(handle) {
            self.drm_display.set_mode_with_framebuffer(fb.handle());
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
