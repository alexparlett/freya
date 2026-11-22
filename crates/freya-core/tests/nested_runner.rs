use std::{
    cell::RefCell,
    rc::Rc,
};

use freya_core::{
    integration::*,
    prelude::*,
};

/// A runner driven inside another's render, as an element drawn offscreen is, hands the
/// outer render its context back, so hooks after it still work.
#[test]
fn a_runner_inside_a_render_restores_the_outer_context() {
    let seen = Rc::new(RefCell::new(None));
    let out = seen.clone();
    let mut runner = Runner::new(move || {
        let mut inner = Runner::new(|| rect().into());
        let _ = inner.sync_and_update();
        drop(inner);
        let count = use_state(|| 7);
        *out.borrow_mut() = Some(*count.read());
        rect().into()
    });
    let _ = runner.sync_and_update();
    assert_eq!(*seen.borrow(), Some(7));
}
