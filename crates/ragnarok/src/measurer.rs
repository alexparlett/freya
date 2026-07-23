use itertools::Itertools;

use crate::{
    Area,
    CursorPoint,
    EmmitableEvent,
    NameOfEvent,
    NodeKey,
    NodesState,
    ProcessedEvents,
    SourceEvent,
    measure_emmitable_events,
    measure_potential_events,
    measure_source_global_events,
};

pub trait EventsMeasurer
where
    Self: std::marker::Sized,
{
    type Name: NameOfEvent;
    type Key: NodeKey;
    type Emmitable: EmmitableEvent<Key = Self::Key, Name = Self::Name>;
    type Source: SourceEvent<Name = Self::Name>;

    fn get_layers(&self) -> impl Iterator<Item = (&i16, impl Iterator<Item = &Self::Key>)>;
    fn get_listeners_of(&self, name: &Self::Name) -> impl Iterator<Item = &Self::Key>;

    fn is_point_inside(&self, key: &Self::Key, cursor: CursorPoint) -> bool;
    fn is_node_parent_of(&self, key: &Self::Key, parent: Self::Key) -> bool;
    fn is_listening_to(&self, key: &Self::Key, name: &Self::Name) -> bool;
    fn is_node_transparent(&self, key: &Self::Key) -> bool;
    fn is_node_interactive(&self, key: &Self::Key) -> bool;

    /// Total order of two node keys by document position (pre-order: ancestors before
    /// descendants, siblings by child index). Used to emit same-name global events in a
    /// deterministic order regardless of listener registration order. Defaults to
    /// [`std::cmp::Ordering::Equal`], which preserves insertion order.
    fn document_order(&self, _a: &Self::Key, _b: &Self::Key) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }

    fn try_area_of(&self, key: &Self::Key) -> Option<Area>;

    fn new_emmitable_event(
        &self,
        key: Self::Key,
        name: Self::Name,
        source: Self::Source,
        area: Option<Area>,
    ) -> Self::Emmitable;
}

impl<T: EventsMeasurer> private::Sealed for T {}

impl<T: EventsMeasurer + private::Sealed> EventsMeasurerRunner for T {
    type Name = T::Name;
    type Key = T::Key;
    type Emmitable = T::Emmitable;
    type Source = T::Source;

    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn run(
        &mut self,
        source_events: &mut Vec<Self::Source>,
        nodes_state: &mut NodesState<Self::Key>,
        focus_id: Option<Self::Key>,
    ) -> ProcessedEvents<Self::Key, Self::Name, Self::Emmitable, Self::Source> {
        // Get potential events that could be emitted based on the elements layout and viewports
        let potential_events = measure_potential_events::<
            Self::Key,
            Self::Name,
            Self::Source,
            Self::Emmitable,
        >(source_events, self, focus_id);

        // Get what events can be actually emitted based on what elements are listening
        let mut emmitable_events =
            measure_emmitable_events::<Self::Key, Self::Name, Self::Source, Self::Emmitable>(
                &potential_events,
                self,
            );

        // Get potential collateral events, e.g. mousemove -> mouseenter
        let collateral_emmitable_events =
            nodes_state.retain_states(self, &emmitable_events, source_events);
        nodes_state.filter_emmitable_events::<Self::Emmitable, Self::Name>(&mut emmitable_events);
        let nodes_states_update =
            nodes_state.create_update::<Self::Name, Self::Source>(self, &potential_events);

        // Get the global events
        measure_source_global_events::<Self::Key, Self::Name, Self::Source, Self::Emmitable>(
            self,
            source_events,
            &mut emmitable_events,
        );
        // Join all the emmitable events and sort them. The sort is stable and same-name
        // global events tie-break by document position, so multiple listeners of the same
        // global event fire in a deterministic pre-order — which is what lets an earlier
        // listener consume the event (via `prevent_default`) before later ones see it.
        emmitable_events.extend(collateral_emmitable_events);
        emmitable_events.sort_by(|a, b| {
            a.cmp(b).then_with(|| {
                if a.name().is_global() {
                    self.document_order(&a.key(), &b.key())
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });

        let mut flattened_potential_events = potential_events.into_values().flatten().collect_vec();
        flattened_potential_events.sort_unstable();

        // Clear the source events vec as all events have been processed
        source_events.clear();

        ProcessedEvents {
            emmitable_events,
            flattened_potential_events,
            nodes_states_update,
        }
    }
}

pub trait EventsMeasurerRunner
where
    Self: std::marker::Sized,
{
    type Name: NameOfEvent;
    type Key: NodeKey;
    type Emmitable: EmmitableEvent<Key = Self::Key, Name = Self::Name>;
    type Source: SourceEvent<Name = Self::Name>;

    fn run(
        &mut self,
        source_events: &mut Vec<Self::Source>,
        nodes_state: &mut NodesState<Self::Key>,
        focus_id: Option<Self::Key>,
    ) -> ProcessedEvents<Self::Key, Self::Name, Self::Emmitable, Self::Source>;
}

#[doc(hidden)]
mod private {
    pub trait Sealed {}
}
