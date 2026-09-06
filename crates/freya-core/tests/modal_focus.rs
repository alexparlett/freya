use freya_core::prelude::*;
use freya_testing::prelude::*;
use std::time::Duration;

type Handles = (State<bool>, State<Vec<&'static str>>);

fn app() -> impl IntoElement {
    let (open, mut log) = use_consume::<Handles>();
    let background = use_a11y();
    rect()
        .child(
            rect()
                .a11y_id(background)
                .a11y_focusable(true)
                .a11y_auto_focus(true)
                .on_key_down(move |_| log.write().push("background")),
        )
        .maybe_child(open().then(|| {
            rect()
                .a11y_modal(true)
                .a11y_focusable(true)
                .on_key_down(move |event: Event<KeyboardEventData>| {
                    log.write().push("modal");
                    if event.key == Key::Character("f".into()) {
                        background.request_focus();
                    }
                })
                .child(rect().a11y_focusable(true))
        }))
}

#[test]
fn modal_contains_navigation_and_explicit_focus_until_removed() {
    let (mut runner, (mut open, mut log)) = TestingRunner::new(
        app,
        (400., 300.).into(),
        |runner| runner.provide_root_context(|| (State::create(false), State::create(Vec::new()))),
        1.,
    );
    runner.poll_n(Duration::from_millis(10), 5);
    runner.press_key(Key::Character("x".into()));
    assert_eq!(&*log.peek(), &["background"]);
    log.write().clear();
    open.set(true);
    runner.poll_n(Duration::from_millis(10), 5);
    runner.press_key(Key::Character("f".into()));
    runner.poll_n(Duration::from_millis(10), 5);
    for modifiers in [Modifiers::empty(), Modifiers::SHIFT] {
        for _ in 0..4 {
            runner.press_key_with_modifiers(Key::Named(NamedKey::Tab), modifiers);
            runner.poll_n(Duration::from_millis(10), 5);
            runner.press_key(Key::Character("x".into()));
        }
    }
    assert!(!log.peek().is_empty());
    assert!(log.peek().iter().all(|entry| *entry == "modal"));
    open.set(false);
    runner.poll_n(Duration::from_millis(10), 5);
    runner.press_key(Key::Named(NamedKey::Tab));
    runner.poll_n(Duration::from_millis(10), 5);
    runner.press_key(Key::Character("x".into()));
    assert_eq!(log.peek().last(), Some(&"background"));
}
