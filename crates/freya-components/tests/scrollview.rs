use std::time::{
    Duration,
    Instant,
};

use freya::prelude::*;
use freya_testing::prelude::*;

#[test]
fn scroll_drag_only_cancels_press_when_content_overflows() {
    for (virtualized, length) in [(false, 1usize), (false, 20), (true, 1), (true, 20)] {
        let (mut test, (presses, controller)) = TestingRunner::new(
            move || {
                let mut presses = use_consume::<State<usize>>();
                let controller = use_consume::<ScrollController>();
                let button = move |index| {
                    Button::new()
                        .key(index)
                        .height(Size::px(150.))
                        .on_press(move |_| *presses.write() += 1)
                        .child(format!("Button {index}"))
                        .into_element()
                };

                if virtualized {
                    VirtualScrollView::new_controlled(move |item, _| button(item.index), controller)
                        .length(length)
                        .item_size(150.)
                        .into_element()
                } else {
                    ScrollView::new_controlled(controller)
                        .children((0..length).map(button))
                        .into_element()
                }
            },
            (500., 500.).into(),
            |runner| {
                let presses = runner.provide_root_context(|| State::create(0usize));
                let controller = runner.provide_root_context(|| ScrollController::new(0., 0.));
                (presses, controller)
            },
            1.,
        );
        test.sync_and_update();

        test.press_touch((40., 80.));
        test.move_touch((40., 50.));
        assert_eq!(test.run_in(|| controller.position().y) > 0., length > 1);
        test.release_touch((40., 50.));
        assert_eq!(*presses.peek(), usize::from(length == 1));
    }
}

#[test]
fn nested_scroll_drag_ownership() {
    for virtualized in [false, true] {
        for (direction, content, start, movement, inner_scrolls) in [
            (
                Direction::Horizontal,
                (200., 100.),
                (0, 0),
                (1., -10.),
                false,
            ),
            (
                Direction::Horizontal,
                (1500., 100.),
                (0, 0),
                (1., -10.),
                false,
            ),
            (
                Direction::Horizontal,
                (1500., 100.),
                (0, 0),
                (-10., -1.),
                true,
            ),
            (Direction::Vertical, (200., 1000.), (0, 0), (0., -10.), true),
            (
                Direction::Vertical,
                (200., 1000.),
                (200, 0),
                (0., 10.),
                false,
            ),
            (
                Direction::Vertical,
                (200., 1000.),
                (200, 900),
                (0., -10.),
                false,
            ),
            (
                Direction::Vertical,
                (200., 1000.),
                (200, 900),
                (0., 10.),
                true,
            ),
        ] {
            let (page_start, inner_start) = start;
            let (mut test, (mut page, mut inner)) = TestingRunner::new(
                move || {
                    let (page, inner) = use_consume::<(ScrollController, ScrollController)>();
                    let row = move |index| {
                        if index == 3 {
                            ScrollView::new_controlled(inner)
                                .key(index)
                                .direction(direction)
                                .width(Size::fill())
                                .height(Size::px(100.))
                                .child(
                                    rect()
                                        .width(Size::px(content.0))
                                        .height(Size::px(content.1)),
                                )
                                .into_element()
                        } else {
                            rect()
                                .key(index)
                                .width(Size::fill())
                                .height(Size::px(100.))
                                .into_element()
                        }
                    };
                    if virtualized {
                        VirtualScrollView::new_controlled(move |item, _| row(item.index), page)
                            .length(10usize)
                            .item_size(100.)
                            .into_element()
                    } else {
                        ScrollView::new_controlled(page)
                            .children((0..10).map(row))
                            .into_element()
                    }
                },
                (500., 500.).into(),
                |runner| {
                    runner.provide_root_context(|| {
                        (
                            ScrollController::new(0., page_start as f64),
                            ScrollController::new(0., inner_start as f64),
                        )
                    })
                },
                1.,
            );
            test.sync_and_update();
            test.sync_and_update();
            let position = |test: &TestingRunner, controller: ScrollController| {
                test.run_in(|| {
                    let position = controller.position();
                    (position.x as i32, position.y as i32)
                })
            };

            assert_eq!(position(&test, page), (0, page_start));
            assert_eq!(position(&test, inner), (0, inner_start));

            let press = (400., 350. - page_start as f64);
            test.press_touch(press);
            for step in 1..=3 {
                test.move_touch((
                    press.0 + movement.0 * step as f64,
                    press.1 + movement.1 * step as f64,
                ));
                let expected_inner = if !inner_scrolls {
                    (0, inner_start)
                } else if direction == Direction::Horizontal {
                    (-movement.0 as i32 * step, inner_start)
                } else {
                    (0, inner_start - movement.1 as i32 * step)
                };
                assert_eq!(position(&test, inner), expected_inner);
                assert_eq!(
                    position(&test, page),
                    (
                        0,
                        page_start
                            - if inner_scrolls {
                                0
                            } else {
                                movement.1 as i32 * step
                            }
                    )
                );
            }
            test.release_touch((press.0 + movement.0 * 3., press.1 + movement.1 * 3.));

            test.run_in(|| {
                page.stop();
                inner.stop();
            });
            test.sync_and_update();

            // Start a new drag outside the inner view.
            let inner_before = position(&test, inner);
            let page_before = position(&test, page);
            test.press_touch((400., 450.));
            test.move_touch((370., 450.));
            assert_eq!(position(&test, inner), inner_before);
            assert_eq!(position(&test, page).0, page_before.0);
            test.move_touch((370., 420.));
            assert_eq!(position(&test, inner), inner_before);
            assert_eq!(position(&test, page).1, page_before.1 + 30);
            test.release_touch((370., 420.));
        }
    }
}

#[test]
fn scroll_view_auto_hide_scrollbars() {
    for auto_hide_scrollbars in [true, false] {
        let mut test = launch_test(move || {
            ScrollView::new()
                .auto_hide_scrollbars(auto_hide_scrollbars)
                .scrollbar(move |context: ScrollBarContext| {
                    assert_eq!(context.auto_hide_scrollbars, auto_hide_scrollbars);
                    ScrollBar::new(context).into()
                })
                .child(rect().width(Size::px(1000.)).height(Size::px(1000.)))
        });
        test.sync_and_update();
        let scrollbars = test.find_many(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.relative_layer == 999.into())
                .map(|_| node)
        });
        assert_eq!(scrollbars.len(), 2);
        for scrollbar in &scrollbars {
            assert!(scrollbar.layout().area.width() > 0.);
            assert!(scrollbar.layout().area.height() > 0.);
        }

        test.poll(Duration::from_millis(20), Duration::from_millis(1000));

        for scrollbar in &scrollbars {
            assert_eq!(scrollbar.layout().area.width() > 0., !auto_hide_scrollbars);
            assert_eq!(scrollbar.layout().area.height() > 0., !auto_hide_scrollbars);
            assert_eq!(scrollbar.children().is_empty(), auto_hide_scrollbars);
        }
    }
}

#[test]
fn scroll_view_mixed_auto_hide_scrollbars() {
    fn app() -> impl IntoElement {
        rect()
            .child(
                ScrollView::new().height(Size::percent(50.)).child(
                    rect().spacing(6.).children(
                        (0..30).map(|_| rect().width(Size::fill()).height(Size::px(80.))),
                    ),
                ),
            )
            .child(
                ScrollView::new()
                    .auto_hide_scrollbars(false)
                    .height(Size::percent(50.))
                    .child(rect().horizontal().spacing(6.).children(
                        (0..30).map(|_| rect().width(Size::px(80.)).height(Size::fill())),
                    )),
            )
    }

    let mut test = launch_test(app);
    test.sync_and_update();
    let scrollbars = test.find_many(|node, element| {
        Rect::try_downcast(element)
            .filter(|rect| rect.relative_layer == 999.into())
            .map(|_| node)
    });
    assert_eq!(scrollbars.len(), 2);
    test.move_cursor((495., 20.));
    test.sync_and_update();
    test.poll(Duration::from_millis(20), Duration::from_secs(2));
    for scrollbar in &scrollbars {
        assert!(scrollbar.layout().area.width() > 0.);
        assert!(!scrollbar.children().is_empty());
    }

    test.move_cursor((100., 100.));
    test.sync_and_update();
    test.poll(Duration::from_millis(20), Duration::from_secs(2));

    let visible_scrollbars = scrollbars
        .iter()
        .filter(|scrollbar| scrollbar.layout().area.width() > 0.)
        .collect::<Vec<_>>();
    for scrollbar in &scrollbars {
        if scrollbar.layout().area.width() == 0. {
            assert!(scrollbar.children().is_empty());
        }
    }
    assert_eq!(visible_scrollbars.len(), 1);
    assert!(visible_scrollbars[0].layout().area.width() > 20.);
    assert_eq!(visible_scrollbars[0].layout().area.height(), 20.);
}

#[test]
pub fn scroll_view_wheel() {
    fn scroll_view_wheel_app() -> impl IntoElement {
        ScrollView::new()
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_wheel_app);
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    // Only the first three items are visible
    // Scrollview height is 500 and the user hasn't scrolled yet
    assert!(content[0].is_visible()); // 1. 0   -> 200, 200 < 500
    assert!(content[1].is_visible()); // 2. 200 -> 400, 200 < 500
    assert!(content[2].is_visible()); // 3. 400 -> 600, 400 < 500
    assert!(!content[3].is_visible()); // 4. 600 -> 800, 600 is NOT < 500, which means it is not visible.

    test.scroll((5., 5.), (0., -300.));

    // Only the last three items are visible
    // Scrollview height is 500 but the user has scrolled 300 pixels
    assert!(!content[0].is_visible()); // 1. 0   -> 200, 200 is NOT > 300, which means it is not visible.
    assert!(content[1].is_visible()); // 2. 200 -> 400, 400 > 300
    assert!(content[2].is_visible()); // 3. 400 -> 600, 600 > 300
    assert!(content[3].is_visible()); // 4. 600 -> 800, 800 > 300
}

#[test]
pub fn scroll_view_smooth_scrolling() {
    fn scroll_view_smooth_scrolling_app() -> impl IntoElement {
        ScrollView::new()
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_smooth_scrolling_app);
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    test.send_event(PlatformEvent::Wheel {
        name: WheelEventName::Wheel,
        scroll: (0., -300.).into(),
        cursor: (5., 5.).into(),
        source: WheelSource::Line,
        timestamp: Instant::now(),
        phase: TouchPhase::Moved,
    });
    test.sync_and_update();

    // A line-based wheel scroll animates, so nothing has moved yet
    assert!(content[0].is_visible());
    assert!(!content[3].is_visible());

    test.poll(Duration::from_millis(16), Duration::from_secs(1));

    // The animation has settled on the destination
    assert!(!content[0].is_visible());
    assert!(content[3].is_visible());
}

#[test]
pub fn scroll_view_hover_updates_on_scroll() {
    fn scroll_view_hover_app() -> impl IntoElement {
        let mut hovered = use_state(|| None::<usize>);

        rect()
            .child(
                label()
                    .height(Size::px(100.))
                    .text(format!("hovered {:?}", hovered())),
            )
            .child(
                ScrollView::new()
                    .height(Size::px(400.))
                    .children((0..4).map(|i| {
                        rect()
                            .key(i)
                            .height(Size::px(200.))
                            .width(Size::px(200.))
                            .on_pointer_enter(move |_| hovered.set(Some(i)))
                    })),
            )
    }

    let mut test = launch_test(scroll_view_hover_app);

    let hovered_label = |test: &TestingRunner| {
        test.find(|_, element| Label::try_downcast(element).map(|label| label.text.to_string()))
            .unwrap()
    };

    assert_eq!(hovered_label(&test), "hovered None");

    test.move_cursor((100., 350.));
    test.sync_and_update();
    assert_eq!(hovered_label(&test), "hovered Some(1)");

    // Scrolling moves the third item under the cursor
    test.scroll((100., 350.), (0., -300.));
    assert_eq!(hovered_label(&test), "hovered Some(2)");
}

#[test]
pub fn scroll_view_scrollbar() {
    fn scroll_view_scrollbar_app() -> impl IntoElement {
        ScrollView::new()
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_scrollbar_app);
    test.animation_clock().disable();
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    // Only the first three items are visible
    // Scrollview height is 500 and the user hasn't scrolled yet
    assert!(content[0].is_visible()); // 1. 0   -> 200, 200 < 500
    assert!(content[1].is_visible()); // 2. 200 -> 400, 200 < 500
    assert!(content[2].is_visible()); // 3. 400 -> 600, 400 < 500
    assert!(!content[3].is_visible()); // 4. 600 -> 800, 600 is NOT < 500, which means it is not visible.

    // Simulate the user dragging the scrollbar
    test.move_cursor((495., 20.));
    test.sync_and_update();
    test.press_cursor((495., 20.));
    test.sync_and_update();
    test.move_cursor((495., 320.));
    test.sync_and_update();
    test.release_cursor((495., 320.));
    test.sync_and_update();

    // Only the last three items are visible
    // Scrollview height is 500 but the user has dragged the scrollbar 300 pixels
    assert!(!content[0].is_visible()); // 1. 0   -> 200, 200 is NOT > 300, which means it is not visible.
    assert!(content[1].is_visible()); // 2. 200 -> 400, 400 > 300
    assert!(content[2].is_visible()); // 3. 400 -> 600, 600 > 300
    assert!(content[3].is_visible()); // 4. 600 -> 800, 800 > 300

    // Scroll up with arrows
    for _ in 0..5 {
        test.press_key(Key::Named(NamedKey::ArrowUp));
        test.poll_n(Duration::from_millis(1), 5);
    }

    assert!(content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(!content[3].is_visible());

    // Scroll to the bottom with arrows
    test.press_key(Key::Named(NamedKey::End));
    test.poll_n(Duration::from_millis(1), 5);

    assert!(!content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(content[3].is_visible());
}

#[test]
fn scroll_views_page_and_jump_along_the_scrollable_axis() {
    for virtualized in [false, true] {
        for horizontal in [false, true] {
            let direction = if horizontal {
                Direction::Horizontal
            } else {
                Direction::Vertical
            };
            let (mut test, controller) = TestingRunner::new(
                move || {
                    let controller = use_consume::<ScrollController>();
                    if virtualized {
                        VirtualScrollView::new_controlled(
                            move |item, _| {
                                rect()
                                    .key(item.index)
                                    .width(if horizontal {
                                        Size::px(item.size)
                                    } else {
                                        Size::fill()
                                    })
                                    .height(if horizontal {
                                        Size::fill()
                                    } else {
                                        Size::px(item.size)
                                    })
                                    .into()
                            },
                            controller,
                        )
                        .length(12usize)
                        .item_size(60.)
                        .direction(direction)
                        .into_element()
                    } else {
                        ScrollView::new_controlled(controller)
                            .child(
                                rect()
                                    .width(if horizontal {
                                        Size::px(720.)
                                    } else {
                                        Size::fill()
                                    })
                                    .height(if horizontal {
                                        Size::fill()
                                    } else {
                                        Size::px(720.)
                                    }),
                            )
                            .into_element()
                    }
                },
                (300., 200.).into(),
                |runner| runner.provide_root_context(|| ScrollController::new(0., 0.)),
                1.,
            );
            test.animation_clock().disable();
            test.sync_and_update();

            let thumb = if horizontal { (20., 195.) } else { (295., 20.) };
            test.press_cursor(thumb);
            test.move_cursor(thumb);
            test.release_cursor(thumb);

            let (arrow, line, page) = if horizontal {
                (NamedKey::ArrowRight, 60., 300.)
            } else {
                (NamedKey::ArrowDown, 40., 200.)
            };
            let end = 720. - page;
            let space = Key::Character(" ".into());
            let no_modifiers = Modifiers::empty();
            for (key, modifiers, offset) in [
                (Key::Named(arrow), no_modifiers, line),
                (Key::Named(NamedKey::PageDown), no_modifiers, line + page),
                (Key::Named(NamedKey::PageUp), no_modifiers, line),
                (space.clone(), no_modifiers, line + page),
                (space.clone(), Modifiers::CONTROL, line + page),
                (space, Modifiers::SHIFT, line),
                (Key::Named(NamedKey::End), no_modifiers, end),
                (Key::Named(NamedKey::PageDown), no_modifiers, end),
                (Key::Named(NamedKey::Home), no_modifiers, 0.),
            ] {
                test.send_event(PlatformEvent::Keyboard {
                    name: KeyboardEventName::KeyDown,
                    key,
                    code: Code::Unidentified,
                    modifiers,
                });
                test.sync_and_update();
                let expected = if horizontal {
                    (offset, 0.)
                } else {
                    (0., offset)
                };
                assert_eq!(test.run_in(|| controller.position()), expected.into());
            }
        }
    }
}

#[test]
pub fn scroll_view_drag_scrolling() {
    fn scroll_view_drag_scrolling_app() -> impl IntoElement {
        ScrollView::new()
            .drag_scrolling(true)
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_drag_scrolling_app);
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    // Initial state: first three items visible
    assert!(content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(!content[3].is_visible());

    // Simulate a touch drag: press down on content, drag upward (scroll down)
    test.press_touch((100., 400.));
    test.move_touch((100., 100.));
    test.release_touch((100., 100.));

    // After dragging 300px upward, first item should be hidden and last visible
    assert!(!content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(content[3].is_visible());
}

#[test]
pub fn scroll_view_drag_scrolling_release_stops() {
    fn scroll_view_drag_release_app() -> impl IntoElement {
        ScrollView::new()
            .drag_scrolling(true)
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_drag_release_app);
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    // Drag down a small amount, then release
    test.press_touch((100., 300.));
    test.move_touch((100., 200.));
    test.release_touch((100., 200.));

    // Scrolled 100px: all four items partially in view since content is 800px, viewport 500px
    assert!(content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(!content[3].is_visible());

    // Move touch further after releasing, should NOT scroll further
    test.move_touch((100., 50.));

    // Visibility should remain unchanged since drag ended on release
    assert!(content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(!content[3].is_visible());
}

#[test]
pub fn scroll_view_drag_scrolling_horizontal() {
    fn scroll_view_drag_horizontal_app() -> impl IntoElement {
        ScrollView::new()
            .drag_scrolling(true)
            .horizontal()
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
            .child(rect().height(Size::px(200.)).width(Size::px(200.)))
    }

    let mut test = launch_test(scroll_view_drag_horizontal_app);
    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let content = scrollview.children()[0].children()[0].children();

    // Initial state: first three items visible (viewport 500px wide, items 200px each)
    assert!(content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(!content[3].is_visible());

    // Drag left (scroll right) by 300px
    test.press_touch((400., 100.));
    test.move_touch((100., 100.));
    test.release_touch((100., 100.));

    // After dragging 300px, first item hidden, last item visible
    assert!(!content[0].is_visible());
    assert!(content[1].is_visible());
    assert!(content[2].is_visible());
    assert!(content[3].is_visible());
}

#[test]
pub fn scroll_view_wheel_latching() {
    use std::time::Duration;

    // Longer than the shared wheel-gesture window, so the next event starts a new gesture.
    fn end_gesture() {
        std::thread::sleep(Duration::from_millis(300));
    }

    fn latching_app() -> impl IntoElement {
        // A latched 200px inner scroll view (400px of content) at the top of an outer scroll
        // view whose own content (200 + 3x200 = 800px) overflows the 500px viewport.
        ScrollView::new()
            .child(
                ScrollView::new()
                    .height(Size::px(200.))
                    .latch_wheel()
                    .child(rect().height(Size::px(200.)).width(Size::fill()))
                    .child(rect().height(Size::px(200.)).width(Size::fill())),
            )
            .child(rect().height(Size::px(200.)).width(Size::fill()))
            .child(rect().height(Size::px(200.)).width(Size::fill()))
            .child(rect().height(Size::px(200.)).width(Size::fill()))
    }

    let mut test = launch_device_test(latching_app);
    let outer = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let outer_content = outer.children()[0].children()[0].children();
    let inner_content = outer_content[0].children()[0].children()[0].children();

    // Initial state: the inner view shows only its first item; the outer's last item is
    // below the fold.
    assert!(inner_content[0].is_visible());
    assert!(!inner_content[1].is_visible());
    assert!(!outer_content[3].is_visible());

    // A gesture starting over the inner view (which can move down) latches to it: the inner
    // scrolls to its end, and the surplus of the same gesture must NOT chain to the outer.
    test.scroll_pixels((100., 100.), (0., -200.));
    assert!(inner_content[1].is_visible());
    assert!(!outer_content[3].is_visible());
    test.scroll_pixels((100., 100.), (0., -300.)); // same gesture, inner already at its end
    assert!(!outer_content[3].is_visible());

    // A NEW gesture starting with the inner at its end passes through wholesale, so the
    // outer takes over.
    end_gesture();
    test.scroll_pixels((100., 100.), (0., -300.));
    assert!(outer_content[3].is_visible());

    // Reset: scroll the outer back up (the inner sits off screen above), then latch a new
    // gesture to the inner to bring it back to its top.
    end_gesture();
    test.scroll_pixels((100., 250.), (0., 300.));
    end_gesture();
    test.scroll_pixels((100., 100.), (0., 200.));
    assert!(!inner_content[1].is_visible());
    assert!(!outer_content[3].is_visible());

    // A gesture starting over the OUTER (below the inner) belongs to it: when the scrolled
    // content brings the inner under the pointer mid-gesture, the inner must not steal the
    // gesture even though it could scroll, so the outer keeps moving and the inner stays put.
    end_gesture();
    test.scroll_pixels((100., 250.), (0., -150.));
    test.scroll_pixels((100., 25.), (0., -150.)); // now over the inner, same gesture
    assert!(!inner_content[1].is_visible());
    assert!(outer_content[3].is_visible());
}

/// Every latching view that saw a gesture's first event is a candidate for it, and the innermost
/// one able to move takes it. An outer latching view therefore gets the gesture exactly when the
/// inner one declined it, rather than being excluded for not having been the first to look.
#[test]
pub fn scroll_view_wheel_latching_nested() {
    fn nested_latching_app() -> impl IntoElement {
        // A latched inner view inside a latched outer view, both able to scroll.
        ScrollView::new()
            .latch_wheel()
            .child(
                ScrollView::new()
                    .height(Size::px(200.))
                    .latch_wheel()
                    .child(rect().height(Size::px(200.)).width(Size::fill()))
                    .child(rect().height(Size::px(200.)).width(Size::fill())),
            )
            .child(rect().height(Size::px(200.)).width(Size::fill()))
            .child(rect().height(Size::px(200.)).width(Size::fill()))
            .child(rect().height(Size::px(200.)).width(Size::fill()))
    }

    let mut test = launch_device_test(nested_latching_app);
    let outer = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let outer_content = outer.children()[0].children()[0].children();
    let inner_content = outer_content[0].children()[0].children()[0].children();

    // The inner view can move, so it takes the gesture and the outer never sees it.
    test.scroll_pixels((100., 100.), (0., -200.));
    assert!(inner_content[1].is_visible());
    assert!(!outer_content[3].is_visible());

    // A new gesture with the inner at its end: the inner declines, so the outer takes it.
    std::thread::sleep(Duration::from_millis(300));
    test.scroll_pixels((100., 100.), (0., -300.));
    assert!(outer_content[3].is_visible());
}

#[test]
pub fn scroll_view_scrollbar_smaller_than_thumb() {
    fn small_scroll_view_app() -> impl IntoElement {
        rect()
            .height(Size::px(48.))
            .width(Size::px(200.))
            .child(ScrollView::new().children((0..30).map(|i| {
                label()
                    .key(i)
                    .height(Size::px(20.))
                    .text(format!("Row {i}"))
            })))
    }

    let mut test = launch_test(small_scroll_view_app);
    test.sync_and_update();

    test.move_cursor((100., 24.));
    test.sync_and_update();
    test.scroll((100., 24.), (0., -20.));
    test.sync_and_update();

    let scrollview = test
        .find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(move |_| node)
        })
        .unwrap();
    let scrolled_before = scrollview.children()[0].children()[0].children()[0]
        .layout()
        .area
        .min_y();

    test.move_cursor((192., 5.));
    test.sync_and_update();
    test.press_cursor((192., 5.));
    test.sync_and_update();
    test.move_cursor((192., 30.));
    test.sync_and_update();
    test.release_cursor((192., 30.));
    test.sync_and_update();

    let scrolled_after = scrollview.children()[0].children()[0].children()[0]
        .layout()
        .area
        .min_y();
    assert_eq!(scrolled_before, scrolled_after);
}

/// Acceleration lands on every scroll surface, so it is applied in `scrollviews::shared` rather
/// than per view. Same gesture as `virtual_scroll_view_wheel_acceleration`, on a plain view.
#[test]
pub fn scroll_view_wheel_acceleration() {
    fn scroll_view_acceleration_app() -> impl IntoElement {
        ScrollView::new().children((0..10).map(|_| {
            rect()
                .height(Size::px(200.))
                .width(Size::px(200.))
                .into_element()
        }))
    }

    let first_visible_item = |test: &TestingRunner| {
        let scrollview = test
            .find(|node, element| {
                Rect::try_downcast(element)
                    .filter(|rect| {
                        rect.accessibility.builder.role() == AccessibilityRole::ScrollView
                    })
                    .map(move |_| node)
            })
            .unwrap();
        let content = scrollview.children()[0].children()[0].children();
        content.iter().position(|child| child.is_visible()).unwrap()
    };

    // A line-granularity wheel animates towards its destination, so what is on screen is only
    // the accelerated distance once the smooth scroll has settled.
    let settle = |test: &mut TestingRunner| {
        test.poll(Duration::from_millis(16), Duration::from_secs(1));
    };

    // A fast gesture: 53 pixels for the first notch, then the second accelerated to the ceiling
    // and capped at the 500 pixel viewport. 553 pixels in, the item spanning 400 to 600 is the
    // first still on screen.
    let mut test = launch_test(scroll_view_acceleration_app);
    test.sync_and_update();
    let start = Instant::now();
    test.scroll_lines_at((5., 5.), (0., -1.), start);
    test.scroll_lines_at((5., 5.), (0., -1.), start + Duration::from_millis(15));
    settle(&mut test);
    assert_eq!(first_visible_item(&test), 2);

    // The same two notches at a reading pace: 106 pixels, so the first item is still on screen.
    let mut test = launch_test(scroll_view_acceleration_app);
    test.sync_and_update();
    let start = Instant::now();
    test.scroll_lines_at((5., 5.), (0., -1.), start);
    test.scroll_lines_at((5., 5.), (0., -1.), start + Duration::from_millis(90));
    settle(&mut test);
    assert_eq!(first_visible_item(&test), 0);
}

/// A runner whose wheel events are shaped as a desktop device's, without the smoothing a Linux
/// touchpad gesture gets, so a test can read where a wheel event landed straight away.
fn launch_device_test(app: impl Into<freya_core::integration::AppComponent>) -> TestingRunner {
    TestingRunner::new(
        app,
        (500., 500.).into(),
        |runner| runner.provide_root_context(|| TargetPlatform::Windows),
        1.,
    )
    .0
}
