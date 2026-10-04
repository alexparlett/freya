use std::borrow::Cow;

use freya_engine::prelude::{
    FontCollection,
    FontMgr,
    SkData,
    TypefaceFontProvider,
    register_font_typeface,
};

/// The fonts every surface of one host shares: the system's, plus any registered at runtime.
pub struct Fonts {
    provider: TypefaceFontProvider,
    pub manager: FontMgr,
    pub collection: FontCollection,
    /// Families tried in order when an element names none.
    pub default_families: Vec<Cow<'static, str>>,
}

impl Fonts {
    /// An empty `default_families` takes Freya's own list.
    pub fn new(mut default_families: Vec<Cow<'static, str>>) -> Self {
        if default_families.is_empty() {
            default_families = freya_core::integration::default_fonts();
        }
        let provider = TypefaceFontProvider::new();
        let manager: FontMgr = provider.clone().into();
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(FontMgr::default(), None);
        collection.set_dynamic_font_manager(manager.clone());
        collection.paragraph_cache_mut().turn_on(false);
        Self {
            provider,
            manager,
            collection,
            default_families,
        }
    }

    /// Registers a font under `name`. Text already measured keeps its old shaping until the
    /// surfaces that show it are told to re-measure ([`crate::Embedded::invalidate_text`]).
    pub fn register(&mut self, name: &str, data: &[u8]) -> bool {
        let Some(typeface) = FontMgr::default().new_from_data(SkData::new_copy(data), None) else {
            tracing::error!("Failed to load the font {name}.");
            return false;
        };
        register_font_typeface(&mut self.provider, name, typeface);
        self.collection.clear_caches();
        true
    }
}
