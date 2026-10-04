use std::{
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
    task::{
        Wake,
        Waker,
    },
    time::{
        Duration,
        Instant,
    },
};

use freya::prelude::*;
use freya_embed::{
    AppComponent,
    EmbedConfig,
    Embedded,
    Fonts,
    KeyboardEventName,
    PlatformEvent,
    RasterTarget,
};

#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn start(
    app: impl Into<AppComponent>,
    size: (f32, f32),
    scale: f64,
    provide: impl FnOnce(&mut freya_embed::Runner),
) -> (Embedded, Fonts, Arc<Wakes>) {
    let wakes = Arc::new(Wakes::default());
    let mut fonts = Fonts::new(vec![]);
    let embedded = Embedded::new(
        EmbedConfig {
            app: app.into(),
            size: size.into(),
            scale_factor: scale,
            name: "test".into(),
            waker: Waker::from(wakes.clone()),
            preferred_theme: PreferredTheme::Dark,
            clipboard: None,
        },
        &mut fonts,
        provide,
    );
    (embedded, fonts, wakes)
}

fn draw(embedded: &mut Embedded, fonts: &mut Fonts, size: (i32, i32)) -> RasterTarget {
    let mut target = RasterTarget::new(size).unwrap();
    embedded.render(fonts, target.surface().canvas());
    target
}

#[test]
fn draws_at_its_scale() {
    let (mut embedded, mut fonts, _) = start(
        || {
            rect()
                .width(Size::px(20.))
                .height(Size::px(10.))
                .background((255, 0, 0))
        },
        (100., 100.),
        2.0,
        |_| {},
    );
    assert!(embedded.update(&mut fonts).redraw);
    let mut target = draw(&mut embedded, &mut fonts, (100, 100));
    assert_eq!(target.pixel(39, 19), [255, 0, 0, 255]);
    assert_eq!(target.pixel(41, 19)[3], 0);
    assert!(!embedded.needs_render());
}

#[test]
fn clicks_reach_handlers() {
    fn app() -> impl IntoElement {
        let mut count = use_consume::<State<i32>>();
        rect()
            .expanded()
            .on_mouse_up(move |_| *count.write() += 1)
            .child(format!("{}", count.read()))
    }
    let mut count = None;
    let (mut embedded, mut fonts, _) = start(app, (50., 50.), 1.0, |runner| {
        count = Some(runner.provide_root_context(|| State::create(0)));
    });
    let count = count.unwrap();
    embedded.update(&mut fonts);
    embedded.click((10., 10.));
    let update = embedded.update(&mut fonts);
    assert_eq!(*count.peek(), 1);
    assert!(update.redraw);
}

#[test]
fn keys_reach_the_focused_node_and_tab_moves_focus() {
    #[derive(Clone, Copy)]
    struct Ids([AccessibilityId; 2]);
    fn app() -> impl IntoElement {
        let ids = use_consume::<Ids>();
        let mut typed = use_consume::<State<String>>();
        rect()
            .child(
                rect()
                    .a11y_id(ids.0[0])
                    .a11y_focusable(true)
                    .a11y_auto_focus(true)
                    .on_key_down(move |e: Event<KeyboardEventData>| {
                        if let Key::Character(c) = &e.key {
                            typed.write().push_str(c);
                        }
                    })
                    .child("first"),
            )
            .child(
                rect()
                    .a11y_id(ids.0[1])
                    .a11y_focusable(true)
                    .child("second"),
            )
    }
    let mut ids = None;
    let mut typed = None;
    let (mut embedded, mut fonts, _) = start(app, (100., 100.), 1.0, |runner| {
        ids = Some(runner.provide_root_context(|| {
            Ids([AccessibilityId::new_unique(), AccessibilityId::new_unique()])
        }));
        typed = Some(runner.provide_root_context(|| State::create(String::new())));
    });
    let (ids, typed) = (ids.unwrap(), typed.unwrap());
    embedded.update(&mut fonts);
    assert_eq!(embedded.focused(), ids.0[0]);

    let key = |embedded: &mut Embedded, key: Key, code: Code| {
        for name in [KeyboardEventName::KeyDown, KeyboardEventName::KeyUp] {
            embedded.event(PlatformEvent::Keyboard {
                name,
                key: key.clone(),
                code,
                modifiers: Modifiers::empty(),
            });
        }
    };
    key(&mut embedded, Key::Character("a".into()), Code::KeyA);
    embedded.update(&mut fonts);
    assert_eq!(*typed.peek(), "a");

    key(&mut embedded, Key::Named(NamedKey::Tab), Code::Tab);
    embedded.update(&mut fonts);
    assert_eq!(embedded.focused(), ids.0[1]);
}

#[test]
fn a_timer_wakes_the_host() {
    fn app() -> impl IntoElement {
        let mut done = use_consume::<State<bool>>();
        use_hook(move || {
            spawn(async move {
                async_io::Timer::after(Duration::from_millis(30)).await;
                done.set(true);
            })
        });
        rect().child(if done() { "done" } else { "waiting" })
    }
    let mut done = None;
    let (mut embedded, mut fonts, wakes) = start(app, (100., 40.), 1.0, |runner| {
        done = Some(runner.provide_root_context(|| State::create(false)));
    });
    let done = done.unwrap();
    embedded.update(&mut fonts);
    let before = wakes.0.load(Ordering::SeqCst);
    let started = Instant::now();
    while wakes.0.load(Ordering::SeqCst) == before {
        assert!(started.elapsed() < Duration::from_secs(2), "never woken");
        std::thread::sleep(Duration::from_millis(5));
    }
    let update = embedded.update(&mut fonts);
    assert!(*done.peek());
    assert!(update.redraw);
}

#[test]
fn a_still_app_does_not_wake() {
    let (mut embedded, mut fonts, wakes) = start(|| rect().child("still"), (50., 50.), 1.0, |_| {});
    embedded.update(&mut fonts);
    draw(&mut embedded, &mut fonts, (50, 50));
    embedded.presented();
    embedded.update(&mut fonts);
    let before = wakes.0.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(wakes.0.load(Ordering::SeqCst), before);
    assert!(!embedded.update(&mut fonts).redraw);
}

#[test]
fn the_accessibility_tree_names_controls() {
    let (mut embedded, mut fonts, _) = start(
        || {
            rect()
                .a11y_role(AccessibilityRole::Button)
                .a11y_focusable(true)
                .child("Press")
        },
        (100., 40.),
        1.0,
        |_| {},
    );
    embedded.update(&mut fonts);
    let snapshot = embedded.accessibility_snapshot();
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|(_, node)| node.role() == accesskit::Role::Button)
    );
}

#[test]
fn resizing_relays_out() {
    let (mut embedded, mut fonts, _) = start(
        || rect().expanded().background((0, 0, 255)),
        (10., 10.),
        1.0,
        |_| {},
    );
    embedded.update(&mut fonts);
    embedded.resize((40., 40.).into(), 1.0);
    assert!(embedded.update(&mut fonts).redraw);
    let mut target = draw(&mut embedded, &mut fonts, (40, 40));
    assert_eq!(target.pixel(35, 35), [0, 0, 255, 255]);
}
