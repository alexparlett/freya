use freya_engine::prelude::*;

/// One Skia context on the host's GL context, shared by every surface the host draws.
///
/// The host keeps its GL context current around every call, and puts back its own GL state after
/// a draw: Skia leaves bindings, programs, blending and pixel-store state changed.
#[cfg(feature = "gl")]
pub struct Gpu {
    context: DirectContext,
}

#[cfg(feature = "gl")]
impl Gpu {
    /// Creates the context with GL entry points from `load`, which the host answers from its own
    /// EGL or GLX loader.
    ///
    /// # Safety
    ///
    /// The host's GL context must be current, and must outlive the returned value or be
    /// followed by [`Gpu::abandon`] before it is destroyed.
    pub unsafe fn new(mut load: impl FnMut(&str) -> *const std::ffi::c_void) -> Option<Self> {
        let interface = Interface::new_load_with(|name| {
            // Skia asks for this to tell EGL from GLX; answering null keeps it on plain GL.
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            load(name)
        })?;
        let context = direct_contexts::make_gl(interface, None)?;
        Some(Self { context })
    }

    pub fn context(&mut self) -> &mut DirectContext {
        &mut self.context
    }

    /// Caps the bytes Skia keeps in its resource cache: glyph atlases, layers, gradients.
    pub fn set_cache_limit(&mut self, bytes: usize) {
        self.context.set_resource_cache_limit(bytes);
    }

    /// The bytes held in the resource cache and the limit.
    pub fn cache_usage(&self) -> (usize, usize) {
        let usage = self.context.resource_cache_usage();
        (usage.resource_bytes, self.context.resource_cache_limit())
    }

    /// Frees every cached resource nothing is using, such as the layers of a closed surface.
    pub fn purge(&mut self) {
        self.context
            .purge_unlocked_resources(gpu::PurgeResourceOptions::AllResources);
    }

    /// Stops Skia touching a GL context that is about to go away.
    pub fn abandon(&mut self) {
        self.context.abandon();
    }
}

/// A Skia surface over a framebuffer object the host owns, typically one with a texture as its
/// colour attachment and a stencil renderbuffer for clips.
#[cfg(feature = "gl")]
pub struct GlTarget {
    surface: Surface,
}

#[cfg(feature = "gl")]
impl GlTarget {
    /// `size` is in physical pixels; `stencil_bits` is the stencil attachment's depth, 0 for
    /// none (Skia then draws clips without a stencil, more slowly).
    pub fn new(gpu: &mut Gpu, fbo: u32, size: (i32, i32), stencil_bits: usize) -> Option<Self> {
        let target = backend_render_targets::make_gl(
            (size.0.max(1), size.1.max(1)),
            0,
            stencil_bits,
            FramebufferInfo {
                fboid: fbo,
                format: Format::RGBA8.into(),
                ..Default::default()
            },
        );
        let surface = wrap_backend_render_target(
            &mut gpu.context,
            &target,
            SurfaceOrigin::TopLeft,
            ColorType::RGBA8888,
            None,
            Some(&text_props()),
        )?;
        Some(Self { surface })
    }

    pub fn surface(&mut self) -> &mut Surface {
        &mut self.surface
    }
}

/// A Skia surface in memory, for tests and for hosts with no GPU.
pub struct RasterTarget {
    surface: Surface,
}

impl RasterTarget {
    pub fn new(size: (i32, i32)) -> Option<Self> {
        let surface = raster_n32_premul((size.0.max(1), size.1.max(1)))?;
        Some(Self { surface })
    }

    pub fn surface(&mut self) -> &mut Surface {
        &mut self.surface
    }

    /// The pixel at `(x, y)` as premultiplied RGBA.
    pub fn pixel(&mut self, x: i32, y: i32) -> [u8; 4] {
        let mut out = [0u8; 4];
        let info = ImageInfo::new((1, 1), ColorType::RGBA8888, AlphaType::Premul, None);
        self.surface.read_pixels(&info, &mut out, 4, (x, y));
        out
    }
}

/// Text contrast and gamma as `freya-winit` draws on Linux.
#[cfg(feature = "gl")]
fn text_props() -> SurfaceProps {
    SurfaceProps::new_with_text_properties(
        SurfacePropsFlags::default(),
        PixelGeometry::Unknown,
        0.2,
        1.0,
    )
}
