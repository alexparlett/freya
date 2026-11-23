use std::{
    collections::HashMap,
    time::Duration,
};

use freya::{
    prelude::*,
    text_edit::*,
};
use freya_testing::prelude::*;

/// An input over a known font, so a press at a given x lands on a known character.
fn measured_input(value: &'static str) -> TestingRunner {
    measured_input_on(value, TargetPlatform::detect())
}

/// [`measured_input`] as it behaves on `platform`, so a platform's shortcuts can be tested on any
/// other.
fn measured_input_on(value: &'static str, platform: TargetPlatform) -> TestingRunner {
    let (mut utils, _) = TestingRunner::new(
        move || {
            let value = use_state(|| value.to_string());
            Input::new(value).width(Size::px(300.))
        },
        (500., 500.).into(),
        |runner| runner.provide_root_context(|| platform),
        1.0,
    );
    utils.set_fonts(HashMap::from_iter([(
        "NotoSans",
        include_bytes!("../../freya-edit/tests/NotoSans-Regular.ttf").as_slice(),
    )]));
    utils.set_default_fonts(&["NotoSans".into()]);
    utils.sync_and_update();
    utils
}

fn highlights(utils: &mut TestingRunner) -> Option<Vec<(usize, usize)>> {
    utils.find(|_, e| Some(Paragraph::try_downcast(e)?.highlights.clone()))
}

#[test]
pub fn input_double_click_selects_the_word_under_the_pointer() {
    let mut utils = measured_input("hello world");

    // Two presses in the same spot inside the first word.
    utils.click_cursor((20.0, 15.0));
    utils.click_cursor((20.0, 15.0));
    // The press that focuses the input renders before the focus lands, and an unfocused
    // input paints no highlight at all: without this frame the selection is right and
    // the assertion still reads an empty one.
    utils.sync_and_update();
    utils.sync_and_update();

    assert_eq!(highlights(&mut utils), Some(vec![(0, 5)]));

    // A third press widens to the line.
    utils.click_cursor((20.0, 15.0));
    assert_eq!(highlights(&mut utils), Some(vec![(0, 11)]));
}

#[test]
pub fn input_double_click_survives_the_pointer_moving_under_it() {
    let mut utils = measured_input("hello world");

    utils.click_cursor((20.0, 15.0));
    utils.press_cursor((20.0, 15.0));
    utils.sync_and_update();
    utils.sync_and_update();
    assert_eq!(highlights(&mut utils), Some(vec![(0, 5)]));

    // A real double click is never perfectly still. The word the press selected must
    // survive the pointer twitching inside it, rather than collapsing to the pointer.
    utils.move_cursor((21.0, 15.0));
    utils.sync_and_update();
    assert_eq!(highlights(&mut utils), Some(vec![(0, 5)]));

    // Dragging on past the word extends by whole words, never mid-word.
    utils.move_cursor((60.0, 15.0));
    utils.sync_and_update();
    assert_eq!(highlights(&mut utils), Some(vec![(0, 11)]));

    utils.release_cursor((60.0, 15.0));
}

#[test]
pub fn input_jumps_and_selects_to_the_line_bounds() {
    jumps_and_selects_to_the_line_bounds(
        TargetPlatform::Linux,
        (
            Key::Named(NamedKey::Home),
            Key::Named(NamedKey::End),
            Modifiers::empty(),
        ),
    );
}

#[test]
pub fn input_jumps_and_selects_to_the_line_bounds_with_cmd_on_macos() {
    jumps_and_selects_to_the_line_bounds(
        TargetPlatform::MacOs,
        (
            Key::Named(NamedKey::ArrowLeft),
            Key::Named(NamedKey::ArrowRight),
            Modifiers::META,
        ),
    );
}

/// The line jump, given as the keys to the start and the end and the modifier they take.
fn jumps_and_selects_to_the_line_bounds(
    platform: TargetPlatform,
    (to_start, to_end, jump): (Key, Key, Modifiers),
) {
    let mut utils = measured_input_on("hello world", platform);

    // Focus with the caret somewhere in the middle.
    utils.click_cursor((20.0, 15.0));
    utils.sync_and_update();

    utils.press_key_with_modifiers(to_start.clone(), jump | Modifiers::SHIFT);
    assert_eq!(highlights(&mut utils), Some(vec![(1, 0)]));

    utils.press_key_with_modifiers(to_end.clone(), jump | Modifiers::SHIFT);
    assert_eq!(highlights(&mut utils), Some(vec![(1, 11)]));

    // Unshifted, the same chord moves the caret and drops the selection.
    utils.press_key_with_modifiers(to_start, jump);
    assert_eq!(highlights(&mut utils), Some(vec![]));

    // Typing now lands at the line start, which is what the jump was for.
    utils.write_text("X");
    let text = utils.find(|_, e| {
        Paragraph::try_downcast(e).filter(|p| p.spans.iter().any(|s| s.text == "Xhello world"))
    });
    assert!(text.is_some());
}

#[test]
fn input_external_editor_control() {
    fn app() -> impl IntoElement {
        Input::new_with_editor(use_consume::<UseEditable>()).auto_focus(true)
    }

    let (mut test, mut editor) = TestingRunner::new(
        app,
        (300., 300.).into(),
        |runner| {
            runner.provide_root_context(|| {
                UseEditable::create("abcdef".to_string(), EditableConfig::new())
            })
        },
        1.,
    );
    test.sync_and_update();
    editor.editor_mut().write().move_cursor_to(3);
    test.sync_and_update();
    test.write_text("X");
    assert_eq!(editor.editor().peek().committed_text(), "abcXdef");

    editor.editor_mut().write().set_selection((0, 7));
    test.sync_and_update();
    test.write_text("replacement");
    assert_eq!(editor.editor().peek().committed_text(), "replacement");

    {
        let mut editor = editor.editor_mut().write();
        editor.set_selection((11, 3));
        editor.set("😀");
        assert_eq!(editor.get_selection(), Some((2, 2)));
        editor.set("external");
    }
    test.sync_and_update();
    assert!(
        test.find(|_, element| {
            Paragraph::try_downcast(element)
                .filter(|paragraph| paragraph.spans.iter().any(|span| span.text == "external"))
        })
        .is_some()
    );
}

#[test]
fn input_setter_reducer() {
    use freya::radio::*;

    #[derive(Default)]
    struct AppState {
        equation: String,
        result: usize,
        edits: usize,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum AppChannel {
        Equation,
    }

    impl RadioChannel<AppState> for AppChannel {}

    enum AppAction {
        SetEquation(String),
    }

    impl DataReducer for AppState {
        type Channel = AppChannel;
        type Action = AppAction;

        fn reduce(&mut self, action: AppAction) -> ChannelSelection<AppChannel> {
            match action {
                AppAction::SetEquation(equation) => {
                    self.result = equation.len();
                    self.equation = equation;
                    self.edits += 1;
                }
            }

            ChannelSelection::Current
        }
    }

    fn app() -> impl IntoElement {
        use_init_radio_station::<AppState, AppChannel>(AppState::default);
        let mut radio = use_radio(AppChannel::Equation);

        rect()
            .child(
                Input::new(WritableSetter::new(
                    radio.slice_current(|state| &state.equation),
                    move |equation| {
                        radio.apply(AppAction::SetEquation(equation));
                    },
                ))
                .height(Size::px(40.)),
            )
            .child(
                rect()
                    .height(Size::px(40.))
                    .on_press(move |_| {
                        radio.apply(AppAction::SetEquation("external".to_string()));
                    })
                    .child("Replace"),
            )
            .child(format!(
                "{}:{}:{}",
                radio.read().equation,
                radio.read().result,
                radio.read().edits,
            ))
    }

    let mut test = launch_test(app);
    test.click_cursor((15., 15.));
    test.write_text("123");

    assert!(
        test.find(|_, element| {
            Label::try_downcast(element).filter(|label| label.text.as_ref() == "123:3:1")
        })
        .is_some()
    );

    test.click_cursor((15., 60.));

    assert!(
        test.find(|_, element| {
            Paragraph::try_downcast(element)
                .filter(|paragraph| paragraph.spans.iter().any(|span| span.text == "external"))
        })
        .is_some()
    );
    assert!(
        test.find(|_, element| {
            Label::try_downcast(element).filter(|label| label.text.as_ref() == "external:8:2")
        })
        .is_some()
    );
}

#[test]
fn input_setter_transform_and_reject_test() {
    fn app() -> impl IntoElement {
        let mut value = use_state(String::new);

        Input::new(WritableSetter::new(value, move |text: String| {
            if text
                .chars()
                .all(|character| character.is_ascii_alphabetic())
            {
                value.set(text.to_uppercase());
            }
        }))
    }

    let mut test = launch_test(app);
    test.click_cursor((15., 15.));
    test.write_text("a");
    test.write_text("1");

    assert!(
        test.find(|_, element| {
            Paragraph::try_downcast(element)
                .filter(|paragraph| paragraph.spans.iter().any(|span| span.text == "A"))
        })
        .is_some()
    );

    test.press_key(Key::Named(NamedKey::End));
    test.write_text("b");

    assert!(
        test.find(|_, element| {
            Paragraph::try_downcast(element)
                .filter(|paragraph| paragraph.spans.iter().any(|span| span.text == "AB"))
        })
        .is_some()
    );
}

#[test]
fn input_mutable_radio_slice_test() {
    use freya::radio::*;

    #[derive(Default)]
    struct AppState {
        text: String,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum AppChannel {
        Text,
    }

    impl RadioChannel<AppState> for AppChannel {}

    fn app() -> impl IntoElement {
        use_init_radio_station::<AppState, AppChannel>(AppState::default);
        let radio = use_radio(AppChannel::Text);

        rect()
            .child(Input::new(radio.slice_mut_current(|state| &mut state.text)))
            .child(format!("text={}", radio.read().text))
    }

    let mut test = launch_test(app);
    test.click_cursor((15., 15.));
    test.write_text("radio");

    assert!(
        test.find(|_, element| {
            Label::try_downcast(element).filter(|label| label.text.as_ref() == "text=radio")
        })
        .is_some()
    );
}

#[test]
pub fn input_test() {
    fn input_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .spacing(6.)
            .child(Input::new(value).placeholder("Type your name"))
            .child(format!("Your name is {}", value.read()))
    }

    let mut test = launch_test(input_app);

    let placeholder = test.find(|_, element| {
        Paragraph::try_downcast(element)
            .filter(|paragraph| paragraph.spans.iter().any(|s| s.text == "Type your name"))
    });
    assert!(placeholder.is_some());
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "Your name is ")
    });
    assert!(label.is_some());

    // Focus
    test.click_cursor((15.0, 15.0));
    // Type
    test.write_text("Rust");

    let text = test.find(|_, element| {
        Paragraph::try_downcast(element)
            .filter(|paragraph| paragraph.spans.iter().any(|s| s.text == "Rust"))
    });
    assert!(text.is_some());
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "Your name is Rust")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_password_mode_test() {
    fn password_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .child(Input::new(value).mode(InputMode::new_password()))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(password_app);

    // Focus and type
    test.click_cursor((15.0, 15.0));
    test.write_text("secret");

    // The rendered paragraph should show masked characters
    let masked = test.find(|_, element| {
        Paragraph::try_downcast(element)
            .filter(|paragraph| paragraph.spans.iter().any(|s| s.text == "******"))
    });
    assert!(masked.is_some());

    // But the underlying state should hold the real value
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=secret")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_validator_test() {
    fn validator_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .child(
                Input::new(value).on_validate(move |validator: InputValidator| {
                    // Only allow numeric input
                    if !validator.text().chars().all(|c| c.is_ascii_digit()) {
                        validator.set_valid(false);
                    }
                }),
            )
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(validator_app);

    // Focus
    test.click_cursor((15.0, 15.0));

    // Type invalid text (letters), should be rejected
    test.write_text("abc");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=")
    });
    assert!(label.is_some());

    // Type valid text (digits), should be accepted
    test.write_text("123");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=123")
    });
    assert!(label.is_some());
}

#[test]
fn input_clipboard_paste_updates_value_and_validates() {
    use freya::clipboard::Clipboard;
    use freya_core::events::modifiers::ModifiersExt;

    fn app() -> impl IntoElement {
        let value = use_state(String::new);
        rect()
            .child(
                Input::new(value)
                    .on_pre_key_down(move |event: Event<KeyboardEventData>| {
                        if matches!(&event.key, Key::Character(character) if character == "v") {
                            let contents = if value.peek().is_empty() {
                                "123"
                            } else {
                                "invalid"
                            };
                            Clipboard::set_text_deferred(contents.to_string());
                        }
                        true
                    })
                    .on_validate(move |validator: InputValidator| {
                        validator.set_valid(
                            validator
                                .text()
                                .chars()
                                .all(|character| character.is_ascii_digit()),
                        );
                    }),
            )
            .child(format!("value={}", value.read()))
    }

    let mut runner = launch_test(app);
    runner.click_cursor((15., 15.));
    let shortcut = |runner: &mut TestingRunner, character: &str| {
        runner.send_event(PlatformEvent::Keyboard {
            name: KeyboardEventName::KeyDown,
            key: Key::Character(character.to_string()),
            code: Code::Unidentified,
            modifiers: Modifiers::ctrl_or_meta(),
        });
        runner.sync_and_update();
        runner.handle_events_immediately();
        runner.sync_and_update();
    };
    shortcut(&mut runner, "v");
    assert!(
        runner
            .find(|_, element| {
                Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=123")
            })
            .is_some()
    );

    shortcut(&mut runner, "a");
    shortcut(&mut runner, "v");
    assert!(
        runner
            .find(|_, element| {
                Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=123")
            })
            .is_some()
    );
    assert!(
        runner
            .find(|_, element| {
                Paragraph::try_downcast(element)
                    .filter(|paragraph| paragraph.spans.iter().any(|span| span.text == "123"))
            })
            .is_some()
    );
}

#[test]
pub fn input_submit_test() {
    fn submit_app() -> impl IntoElement {
        let value = use_state(String::new);
        let mut submitted = use_state(String::new);

        rect()
            .child(Input::new(value).on_submit(move |text: String| {
                submitted.set(text);
            }))
            .child(format!("submitted={}", submitted.read()))
    }

    let mut test = launch_test(submit_app);

    // Focus and type
    test.click_cursor((15.0, 15.0));
    test.write_text("hello");

    // Not yet submitted
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "submitted=")
    });
    assert!(label.is_some());

    // Press Enter to submit
    test.press_key(Key::Named(NamedKey::Enter));

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "submitted=hello")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_multiline_test() {
    fn multiline_app() -> impl IntoElement {
        let value = use_state(String::new);
        let mut submitted = use_state(|| false);

        rect()
            .child(
                Input::new(value)
                    .multiline(true)
                    .on_submit(move |_| submitted.set(true)),
            )
            .child(format!(
                "value={:?} submitted={}",
                value.read(),
                submitted.read()
            ))
    }

    let mut test = launch_test(multiline_app);

    // Focus and type two lines. With an `on_submit` in place plain Enter submits, so the line
    // break is `Shift`+`Enter`. See [`Input::multiline`].
    test.click_cursor((15.0, 15.0));
    test.write_text("hello");
    test.press_key_with_modifiers(Key::Named(NamedKey::Enter), Modifiers::SHIFT);
    test.write_text("world");

    // Shift+Enter inserted a line break instead of submitting
    let label = test.find(|_, element| {
        Label::try_downcast(element)
            .filter(|label| label.text.as_ref() == "value=\"hello\\nworld\" submitted=false")
    });
    assert!(label.is_some());
}

/// A multiline input left at its default height hugs its text between `min_height` and
/// `max_height`, and past the cap the text scrolls inside it rather than the box growing.
#[test]
pub fn input_multiline_grows_with_its_text_up_to_max_height() {
    fn multiline_app() -> impl IntoElement {
        let value = use_state(String::new);
        rect().child(
            Input::new(value)
                .multiline(true)
                .min_height(20.)
                .max_height(80.),
        )
    }

    fn scroll_view_area(test: &TestingRunner) -> Area {
        test.find(|node, element| {
            Rect::try_downcast(element)
                .filter(|rect| rect.accessibility.builder.role() == AccessibilityRole::ScrollView)
                .map(|_| node.layout().area)
        })
        .expect("the input's scroll view")
    }

    let mut test = launch_test(multiline_app);
    test.click_cursor((15.0, 15.0));
    test.write_text("one");
    let one_line = scroll_view_area(&test).height();
    assert!(one_line > 0. && one_line < 80., "one line hugs: {one_line}");

    for _ in 0..10 {
        test.press_key(Key::Named(NamedKey::Enter));
        test.write_text("more");
    }
    let many_lines = scroll_view_area(&test).height();
    assert_eq!(many_lines, 80., "the box stops at max_height");
}

#[test]
fn input_multiline_unicode_navigation() {
    fn app() -> impl IntoElement {
        Input::new_with_editor(use_consume::<UseEditable>())
            .multiline(true)
            .width(Size::px(500.))
            .height(Size::px(400.))
            .auto_focus(true)
    }

    for text in [
        "Hello, world!\nThis is an editable multiline text box.\n\n한국어도 돼요\n\nok what\nis going on",
        "Hello\n한국어도 돼요\n\n🦀🦀\n한글\nok\n",
    ] {
        let (mut test, editor) = TestingRunner::new(
            app,
            (500., 400.).into(),
            |runner| {
                runner.provide_root_context(|| {
                    UseEditable::create(text.to_string(), EditableConfig::new())
                })
            },
            1.,
        );
        test.sync_and_update();

        let line_count = editor.editor().peek().len_lines();
        for row in 1..line_count {
            test.press_key(Key::Named(NamedKey::ArrowDown));
            let editor = editor.editor().peek();
            assert_eq!(editor.cursor_row(), row, "moving down in {text:?}");
            assert_eq!(editor.cursor_col(), 0);
        }

        for row in (0..line_count - 1).rev() {
            test.press_key(Key::Named(NamedKey::ArrowUp));
            let editor = editor.editor().peek();
            assert_eq!(editor.cursor_row(), row, "moving up in {text:?}");
            assert_eq!(editor.cursor_col(), 0);
        }
    }
}

#[test]
pub fn input_multiline_fixed_height_follows_cursor_test() {
    fn multiline_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect().child(
            Input::new(value)
                .multiline(true)
                .width(Size::px(300.))
                .height(Size::px(60.)),
        )
    }

    let mut test = launch_test(multiline_app);
    test.click_cursor((15.0, 15.0));

    for line in 1..=6 {
        test.write_text("A line of text");
        test.press_key(Key::Named(NamedKey::Enter));
        test.sync_and_update();
        test.sync_and_update();

        if line >= 5 {
            let (paragraph_y, paragraph_height) = test
                .find(|node, element| {
                    Paragraph::try_downcast(element)
                        .map(|_| (node.layout().area.min_y(), node.layout().area.height()))
                })
                .expect("input paragraph should be present");
            assert!(
                paragraph_y < 0.0,
                "paragraph should scroll when text overflows"
            );
            assert!(
                paragraph_y + paragraph_height - 8.0 <= 61.0,
                "cursor line should remain visible, paragraph_y={paragraph_y} paragraph_height={paragraph_height}"
            );
        }
    }
}

#[test]
pub fn input_multiline_scrollbar_press_test() {
    fn scrollbar_app() -> impl IntoElement {
        let value = use_state(|| "One\nTwo\nThree\nFour\nFive\nSix\nSeven".to_string());

        rect().child(
            Input::new(value)
                .multiline(true)
                .width(Size::px(300.))
                .height(Size::px(120.)),
        )
    }

    let mut test = launch_test(scrollbar_app);
    test.sync_and_update();

    test.click_cursor((100.0, 40.0));
    test.press_cursor((292.0, 30.0));
    test.move_cursor((100.0, 90.0));
    test.sync_and_update();

    let highlights =
        test.find(|_, element| Some(Paragraph::try_downcast(element)?.highlights.clone()));
    assert_eq!(highlights, Some(vec![]));
}

#[test]
pub fn input_disabled_test() {
    fn disabled_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .child(Input::new(value).enabled(false))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(disabled_app);

    // Try to focus and type
    test.click_cursor((15.0, 15.0));
    test.write_text("hello");

    // Value should remain empty since input is disabled
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_escape_unfocus_test() {
    fn escape_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .child(Input::new(value))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(escape_app);

    // Focus and type
    test.click_cursor((15.0, 15.0));
    test.write_text("hello");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=hello")
    });
    assert!(label.is_some());

    // Press Escape to unfocus.
    test.press_key(Key::Named(NamedKey::Escape));
    test.sync_and_update();

    // Type more text, should not be captured since the input lost focus
    test.write_text("world");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=hello")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_shift_wheel_scroll_test() {
    fn scroll_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect().child(Input::new(value).width(Size::px(150.)))
    }

    let mut test = launch_test(scroll_app);

    // Focus and fill with text wider than the input
    test.click_cursor((15.0, 15.0));
    test.write_text("this is a very long text that overflows the input width");
    test.sync_and_update();
    test.sync_and_update();

    let paragraph_x = |test: &TestingRunner| {
        test.find(|node, element| {
            Paragraph::try_downcast(element).map(|_| node.layout().area.min_x())
        })
        .unwrap()
    };

    // Move the cursor back to the start so the input is scrolled to the beginning
    test.press_key(Key::Named(NamedKey::Home));
    test.sync_and_update();
    let initial_x = paragraph_x(&test);

    // Hold Shift and wheel over the input to scroll it horizontally while focused
    test.send_event(PlatformEvent::Keyboard {
        name: KeyboardEventName::KeyDown,
        key: Key::Named(NamedKey::Shift),
        code: Code::ShiftLeft,
        modifiers: Modifiers::SHIFT,
    });
    test.sync_and_update();

    test.send_event(PlatformEvent::Wheel {
        name: WheelEventName::Wheel,
        scroll: (0.0, -50.0).into(),
        cursor: (75.0, 15.0).into(),
        source: WheelSource::Line,
        timestamp: std::time::Instant::now(),
        phase: TouchPhase::Moved,
    });
    test.sync_and_update();

    test.poll(Duration::from_millis(16), Duration::from_millis(500));

    let scrolled_x = paragraph_x(&test);
    assert!(scrolled_x < initial_x);
}

#[test]
pub fn input_drag_scrolls_to_cursor_test() {
    fn drag_app() -> impl IntoElement {
        let value =
            use_state(|| "this is a very long text that overflows the input width".to_string());

        rect().child(Input::new(value).width(Size::px(150.)))
    }

    let mut test = launch_test(drag_app);
    test.sync_and_update();

    let paragraph_x = |test: &TestingRunner| {
        test.find(|node, element| {
            Paragraph::try_downcast(element).map(|_| node.layout().area.min_x())
        })
        .unwrap()
    };
    let initial_x = paragraph_x(&test);

    test.press_cursor((100.0, 15.0));
    test.sync_and_update();
    test.move_cursor((170.0, 15.0));
    test.sync_and_update();

    let dragged_x = paragraph_x(&test);
    assert!(dragged_x < initial_x);
    assert!(initial_x - dragged_x < 100.0);

    // Dragging back past the left edge scrolls back to the beginning
    test.move_cursor((-500.0, 15.0));
    test.sync_and_update();

    assert_eq!(paragraph_x(&test), initial_x);

    test.release_cursor((-500.0, 15.0));
}

#[test]
pub fn input_auto_focus_test() {
    fn auto_focus_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect()
            .child(Input::new(value).auto_focus(true))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(auto_focus_app);

    test.sync_and_update();

    // Type without clicking, auto_focus should have focused the input
    test.write_text("typed");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=typed")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_select_all_on_init_test() {
    fn rename_app() -> impl IntoElement {
        // Seeded before the input mounts, the way a rename affordance opens over the name it is
        // replacing.
        let value = use_state(|| "old name".to_string());

        rect()
            .child(Input::new(value).auto_focus(true).select_all_on_init(true))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(rename_app);

    test.sync_and_update();

    // No click needed: the input auto-focuses, and the seeded value arrives selected.
    test.write_text("new");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=new")
    });
    assert!(
        label.is_some(),
        "typing replaced the selected value instead of landing in front of it"
    );
}

#[test]
pub fn input_select_on_init_selects_only_that_range() {
    fn rename_app() -> impl IntoElement {
        let value = use_state(|| "café notes.md".to_string());

        rect()
            .child(Input::new(value).auto_focus(true).select_on_init(0..10))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(rename_app);

    test.sync_and_update();

    // The name is replaced and the extension kept, the é counted as one char.
    test.write_text("plan");

    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=plan.md")
    });
    assert!(label.is_some(), "typing replaced only the selected name");
}

#[test]
pub fn input_without_select_all_on_init_keeps_the_cursor_at_the_start() {
    fn seeded_app() -> impl IntoElement {
        let value = use_state(|| "old name".to_string());

        rect()
            .child(Input::new(value).auto_focus(true))
            .child(format!("value={}", value.read()))
    }

    let mut test = launch_test(seeded_app);

    test.sync_and_update();

    test.write_text("new");

    // The default is unchanged: an input mounts with its cursor at position 0, so this inserts
    // there. `select_all_on_init` is opt-in precisely because that is right for a field the user
    // is editing rather than replacing.
    let label = test.find(|_, element| {
        Label::try_downcast(element).filter(|label| label.text.as_ref() == "value=newold name")
    });
    assert!(label.is_some());
}

#[test]
pub fn input_long_typing_follows_cursor_test() {
    fn typing_app() -> impl IntoElement {
        let value = use_state(String::new);

        rect().child(Input::new(value).width(Size::px(150.)))
    }

    let mut test = launch_test(typing_app);

    let paragraph_metrics = |test: &TestingRunner| {
        test.find(|node, element| {
            Paragraph::try_downcast(element)
                .map(|_| (node.layout().area.min_x(), node.layout().area.width()))
        })
        .unwrap()
    };

    test.click_cursor((15.0, 15.0));
    let (initial_x, _) = paragraph_metrics(&test);

    // Keep typing way beyond the input width, the scroll must follow the cursor every time
    for _ in 0..20 {
        test.write_text("some more text ");
        test.sync_and_update();
        test.sync_and_update();

        let (min_x, width) = paragraph_metrics(&test);
        if width > 150.0 {
            let right_edge = min_x + width;
            assert!(
                (right_edge - 150.0).abs() < 2.0,
                "text end should stay at the right edge, min_x={min_x} width={width}"
            );
        }
    }

    // Moving the cursor back to the start scrolls the input back to the beginning
    test.press_key(Key::Named(NamedKey::Home));
    test.sync_and_update();

    let (home_x, _) = paragraph_metrics(&test);
    assert_eq!(home_x, initial_x);
}
