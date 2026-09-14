use freya_core::prelude::*;
use torin::size::Size;

use crate::{
    define_theme,
    get_theme,
    svg_viewer::SvgViewer,
};

define_theme! {
    %[component]
    pub TitlebarButton {
        %[fields]
        background: Color,
        hover_background: Color,
        /// The glyph's tint at rest.
        ///
        /// The icon is drawn with `currentColor`, so without this field a caller had to wrap the
        /// button in a coloured parent to tint it — which meant the theme could not state the one
        /// thing a titlebar button is mostly made of.
        color: Color,
        /// **Close**, hovered. The one action here with consequences, and every desktop paints it
        /// red rather than in the hover tone its neighbours use — a titlebar whose close button
        /// highlights like minimize is one people mis-click.
        ///
        /// Separate fields rather than a caller-side special case: [`TitlebarAction`] already
        /// tells this component which button is the destructive one, so where that button differs
        /// is a fact the theme should be able to state.
        close_hover_background: Color,
        close_hover_color: Color,
        corner_radius: CornerRadius,
        width: Size,
        height: Size,
    }
}

#[derive(Clone, PartialEq, Copy)]
pub enum TitlebarAction {
    Minimize,
    Maximize,
    Close,
    Restore,
}

/// Titlebar button component.
#[derive(PartialEq)]
pub struct TitlebarButton {
    pub(crate) theme: Option<TitlebarButtonThemePartial>,
    pub(crate) action: TitlebarAction,
    pub(crate) on_press: Option<EventHandler<Event<PressEventData>>>,
    key: DiffKey,
}

impl KeyExt for TitlebarButton {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl TitlebarButton {
    pub fn new(action: TitlebarAction) -> Self {
        Self {
            theme: None,
            action,
            on_press: None,
            key: DiffKey::None,
        }
    }

    pub fn on_press(mut self, on_press: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_press = Some(on_press.into());
        self
    }

    /// Override this button's theme, like every other themed component here — without it the
    /// `theme` field is unreachable and the registered default is the only dress a titlebar
    /// button can wear.
    pub fn theme(mut self, theme: TitlebarButtonThemePartial) -> Self {
        self.theme = Some(theme);
        self
    }
}

impl Component for TitlebarButton {
    fn render(&self) -> impl IntoElement {
        let mut hovering = use_state(|| false);
        let theme = get_theme!(
            &self.theme,
            TitlebarButtonThemePreference,
            "titlebar_button"
        );

        let icon_svg = match self.action {
            TitlebarAction::Minimize => {
                r#"<svg viewBox="0 0 12 12"><rect x="1" y="5" width="10" height="2" fill="currentColor"/></svg>"#
            }
            TitlebarAction::Maximize => {
                r#"<svg viewBox="0 0 12 12"><rect x="2" y="2" width="8" height="8" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>"#
            }
            TitlebarAction::Restore => {
                r#"<svg viewBox="0 0 12 12" xmlns="http://www.w3.org/2000/svg">
                  <rect x="1.5" y="3.5" width="6.5" height="6.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
                  <path d="M4 3.5 V2 H10 V8 H8.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
                </svg>"#
            }
            TitlebarAction::Close => {
                r#"<svg viewBox="0 0 12 12"><path d="M3 3l6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M9 3l-6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>"#
            }
        };

        let icon = SvgViewer::new(icon_svg.as_bytes())
            .width(Size::px(12.))
            .height(Size::px(12.));

        // Close is the destructive action, so it carries its own hovered pair; the other three
        // share the ordinary one.
        let destructive = matches!(self.action, TitlebarAction::Close);
        let (background, color) = match (hovering(), destructive) {
            (true, true) => (theme.close_hover_background, theme.close_hover_color),
            (true, false) => (theme.hover_background, theme.color),
            (false, _) => (theme.background, theme.color),
        };

        rect()
            .width(theme.width)
            .height(theme.height)
            .background(background)
            .color(color)
            .center()
            .on_pointer_enter(move |_| {
                hovering.set(true);
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
            })
            // A titlebar button sits inside a drag region by definition — that is what a titlebar
            // is. Without this the press that activates the button also starts a window drag (and
            // a double-press maximizes), so the stop belongs here rather than in every caller.
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .map(self.on_press.clone(), |el, on_press| el.on_press(on_press))
            .child(icon)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}
