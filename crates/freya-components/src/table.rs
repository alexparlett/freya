use freya_core::prelude::*;
use torin::{
    content::Content,
    gaps::Gaps,
    node::Node,
    prelude::Alignment,
    size::Size,
};

use crate::{
    define_theme,
    get_theme,
    icons::{
        IconThemePartialExt,
        arrow::ArrowIcon,
    },
    theming::hooks::get_theme_or_default,
};

define_theme! {
    for = Table;
    theme_field = theme;
    for = TableRow;
    theme_field = theme;

    %[component]
    pub Table {
        %[fields]
        background: Color,
        arrow_fill: Color,
        hover_row_background: Color,
        row_background: Color,
        /// The rule between two rows.
        divider_fill: Color,
        /// The table's own outer edge. Separate from `divider_fill` because a table's box and
        /// the rules inside it are different weights in most designs, and a single token can
        /// only be authored for one of them: pitched for the rules, the box disappears; pitched
        /// for the box, every row is banded.
        border_fill: Color,
        corner_radius: CornerRadius,
        color: Color,
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum OrderDirection {
    Up,
    #[default]
    Down,
}

/// An arrow showing the [OrderDirection] a column is sorted in.
#[derive(PartialEq)]
pub struct TableArrow {
    pub order_direction: OrderDirection,
    key: DiffKey,
}

impl TableArrow {
    pub fn new(order_direction: OrderDirection) -> Self {
        Self {
            order_direction,
            key: DiffKey::None,
        }
    }
}

impl KeyExt for TableArrow {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for TableArrow {
    fn render(&self) -> impl IntoElement {
        let TableTheme { arrow_fill, .. } =
            get_theme!(None::<TableThemePartial>, TableThemePreference, "table");
        let rotate = match self.order_direction {
            OrderDirection::Down => 0.,
            OrderDirection::Up => 180.,
        };
        ArrowIcon::new().rotate(rotate).fill(arrow_fill)
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// Resolved theme and column layout a [Table] shares with its rows.
#[derive(Clone, PartialEq)]
pub struct TableConfig {
    pub theme: TableTheme,
    /// The table's own theme override, so a [TableRow] with a theme of its own can layer it on
    /// top rather than on the global theme.
    pub theme_override: Option<TableThemePartial>,
    pub column_widths: Option<Vec<Size>>,
    pub column_aligns: Option<Vec<Alignment>>,
}

/// The live [TableConfig] a [Table] shares with the [TableRow]s under it.
///
/// A [Readable] rather than a plain value, because a table's columns may change while its rows are
/// mounted: `use_try_consume` runs once per component instance, so a row that read a plain context
/// would keep the split it was born with. A row reading through this subscribes, and re-renders
/// when the columns move.
#[derive(Clone)]
pub struct TableConfigContext(pub Readable<TableConfig>);

#[derive(PartialEq)]
pub struct TableRow {
    pub theme: Option<TableThemePartial>,
    /// Optional press handler, called for a press anywhere in the row.
    pub on_press: Option<EventHandler<Event<PressEventData>>>,
    pub children: Vec<Element>,
    layout: LayoutData,
    key: DiffKey,
}

impl Default for TableRow {
    fn default() -> Self {
        Self::new()
    }
}

impl TableRow {
    pub fn new() -> Self {
        Self {
            theme: None,
            on_press: None,
            children: vec![],
            layout: Node {
                width: Size::fill(),
                ..Default::default()
            }
            .into(),
            key: DiffKey::None,
        }
    }

    /// Dress this row on its own, over the table's theme.
    ///
    /// A row is where a table says something about one record: the selected row, a zebra
    /// stripe, an invalid entry. Setting `hover_row_background` to the same fill as
    /// `row_background` also opts the row out of the hover response, for a table whose rows
    /// carry a selection instead.
    pub fn theme(mut self, theme: TableThemePartial) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Handle a press anywhere in the row. Use it for a table whose rows are selectable.
    pub fn on_press(mut self, handler: impl Into<EventHandler<Event<PressEventData>>>) -> Self {
        self.on_press = Some(handler.into());
        self
    }
}

impl LayoutExt for TableRow {
    fn get_layout(&mut self) -> &mut LayoutData {
        &mut self.layout
    }
}

impl ContainerExt for TableRow {}

impl ChildrenExt for TableRow {
    fn get_children(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl KeyExt for TableRow {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl Component for TableRow {
    fn render(&self) -> impl IntoElement {
        let config = use_try_consume::<TableConfigContext>().map(|config| config.0.read().clone());
        let (column_widths, column_aligns) = config
            .as_ref()
            .map(|config| (config.column_widths.clone(), config.column_aligns.clone()))
            .unwrap_or_default();
        let theme = match (&self.theme, config) {
            (None, Some(config)) => config.theme,
            (row_theme, config) => {
                let theme = get_theme_or_default();
                let theme = theme.read();
                let mut preference = theme
                    .get::<TableThemePreference>("table")
                    .cloned()
                    .expect("Theme key not found: table");
                if let Some(table_theme) = config.and_then(|config| config.theme_override) {
                    preference.apply_optional(&table_theme);
                }
                if let Some(row_theme) = row_theme {
                    preference.apply_optional(row_theme);
                }
                preference.resolve(&*theme.palette)
            }
        };
        let mut hovering = use_state(|| false);
        let background = if hovering() {
            theme.hover_row_background
        } else {
            theme.row_background
        };

        rect()
            .layout(self.layout.clone())
            .horizontal()
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .background(background)
            .border(Border::new().fill(theme.divider_fill).width(BorderWidth {
                bottom: 1.,
                ..Default::default()
            }))
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .map(self.on_press.clone(), |el, on_press| {
                el.on_press(move |e| on_press.call(e))
            })
            .children(self.children.iter().enumerate().map(|(index, child)| {
                let width = column_widths
                    .as_ref()
                    .and_then(|widths| widths.get(index).cloned())
                    .unwrap_or_else(|| Size::flex(1.));
                let main_align = column_aligns
                    .as_ref()
                    .and_then(|aligns| aligns.get(index).cloned())
                    .unwrap_or(Alignment::End);

                rect()
                    .width(width)
                    .overflow(Overflow::Clip)
                    .padding(Gaps::new_all(5.0))
                    .horizontal()
                    .main_align(main_align)
                    .cross_align(Alignment::Center)
                    .child(child.clone())
            }))
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

/// A table component with rows and columns.
///
/// # Example
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     Table::new()
///         .child(TableRow::new().child("Header 1").child("Header 2"))
///         .child(TableRow::new().child("Data 1").child("Data 2"))
///         .child(TableRow::new().child("Data 3").child("Data 4"))
/// }
/// ```
///
/// The table lays its children out with flex content, so a table standing at a given height
/// can hand what is left of it to a child: a header row over a scrolling body wants the body
/// at [`Size::flex`]. Inert for the default [`Size::Inner`], where no child asks for a share.
///
/// See the [interactive components demo](https://freyaui.dev/demo).
#[derive(PartialEq)]
pub struct Table {
    pub theme: Option<TableThemePartial>,
    pub column_widths: Option<Vec<Size>>,
    pub column_aligns: Option<Vec<Alignment>>,
    pub children: Vec<Element>,
    layout: LayoutData,
    key: DiffKey,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    pub fn new() -> Self {
        Self {
            theme: None,
            column_widths: None,
            column_aligns: None,
            children: vec![],
            layout: Node {
                content: Content::Flex,
                ..Default::default()
            }
            .into(),
            key: DiffKey::None,
        }
    }

    pub fn theme(mut self, theme: TableThemePartial) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Set custom widths for each column.
    ///
    /// Accepts any [Size], defaults to [Size::Flex].
    pub fn column_widths(mut self, widths: impl Into<Vec<Size>>) -> Self {
        self.column_widths = Some(widths.into());
        self
    }

    /// Set where each column's content sits along its row.
    ///
    /// Defaults to [`Alignment::End`], which suits the numeric columns a table is usually built
    /// from; text columns want [`Alignment::Start`].
    pub fn column_aligns(mut self, aligns: impl Into<Vec<Alignment>>) -> Self {
        self.column_aligns = Some(aligns.into());
        self
    }
}

impl ChildrenExt for Table {
    fn get_children(&mut self) -> &mut Vec<Element> {
        &mut self.children
    }
}

impl KeyExt for Table {
    fn write_key(&mut self) -> &mut DiffKey {
        &mut self.key
    }
}

impl LayoutExt for Table {
    fn get_layout(&mut self) -> &mut LayoutData {
        &mut self.layout
    }
}

impl ContainerExt for Table {}

impl Component for Table {
    fn render(&self) -> impl IntoElement {
        let theme = get_theme!(&self.theme, TableThemePreference, "table");
        let config = TableConfig {
            theme: theme.clone(),
            theme_override: self.theme.clone(),
            column_widths: self.column_widths.clone(),
            column_aligns: self.column_aligns.clone(),
        };
        let mut state = use_state(|| config.clone());
        if *state.peek() != config {
            *state.write() = config;
        }
        use_provide_context(|| TableConfigContext(state.into_readable()));

        rect()
            .layout(self.layout.clone())
            .overflow(Overflow::Clip)
            .color(theme.color)
            .background(theme.background)
            .corner_radius(theme.corner_radius)
            .border(
                Border::new()
                    .alignment(BorderAlignment::Outer)
                    .fill(theme.border_fill)
                    .width(1.0),
            )
            .children(self.children.clone())
    }

    fn render_key(&self) -> DiffKey {
        self.key.clone().or(self.default_key())
    }
}

#[cfg(test)]
mod tests {
    use freya::prelude::*;
    use freya_testing::TestingRunner;

    use crate::table::{
        Table,
        TableRow,
    };

    /// A table whose column widths change while its rows stay mounted re-lays those rows.
    ///
    /// The regression is quiet and permanent: a row reads its split through `use_try_consume`,
    /// which runs once per component instance, so before [TableConfigContext](super::
    /// TableConfigContext) held a `Readable` a row kept the widths it was born with. A table that
    /// starts without widths and gains them (an empty state that fills, a column set that depends
    /// on state) laid its rows out at an equal share for the rest of their lives.
    ///
    /// Measured on the first cell's laid-out width rather than on the element tree, because the
    /// tree was right the whole time.
    #[test]
    fn a_row_follows_column_widths_that_change_under_it() {
        fn app() -> impl IntoElement {
            let widths = consume_context::<State<Option<Vec<Size>>>>();
            Table::new()
                .width(Size::fill())
                .map(widths.read().clone(), |table, widths| {
                    table.column_widths(widths)
                })
                .child(TableRow::new().child("a").child("b"))
        }

        let (mut runner, widths) = TestingRunner::new(
            app,
            (400., 200.).into(),
            |runner| runner.provide_root_context(|| State::create(None::<Vec<Size>>)),
            1.,
        );
        runner.sync_and_update();

        /// The width torin gave the row's first cell wrapper.
        fn first_cell(runner: &TestingRunner) -> f32 {
            runner
                .find_many(|node, _| {
                    let area = node.layout().area;
                    (area.width() > 0. && area.height() > 0.).then(|| (area.min_y(), area.width()))
                })
                .into_iter()
                .filter(|(_, width)| *width < 400.)
                .map(|(_, width)| width)
                .next()
                .expect("a laid-out cell")
        }

        assert_eq!(first_cell(&runner), 200., "two flex columns share the row");

        let mut widths = widths;
        widths.set(Some(vec![Size::px(120.), Size::flex(1.)]));
        runner.sync_and_update();
        runner.sync_and_update();

        assert_eq!(
            first_cell(&runner),
            120.,
            "the row took the split it was given after it had mounted"
        );
    }
}
