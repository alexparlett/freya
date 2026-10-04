use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::AtomicU64,
};

use rustc_hash::FxHashMap;

use crate::{
    prelude::{
        Task,
        TaskId,
    },
    reactive_context::ReactiveContext,
    runner::Message,
    scope::ScopeStorage,
    scope_id::ScopeId,
};

// TODO: rendering flag.
pub struct CurrentContext {
    pub scope_id: ScopeId,
    pub scopes_storages: Rc<RefCell<FxHashMap<ScopeId, ScopeStorage>>>,

    pub tasks: Rc<RefCell<FxHashMap<TaskId, Rc<RefCell<Task>>>>>,
    pub task_id_counter: Rc<AtomicU64>,
    pub sender: futures_channel::mpsc::UnboundedSender<Message>,
}

impl CurrentContext {
    pub fn run_with_reactive<T>(new_context: Self, run: impl FnOnce() -> T) -> T {
        let reactive_context = {
            let scope_storages = new_context.scopes_storages.borrow();
            let scope_storage = scope_storages.get(&new_context.scope_id).unwrap();
            scope_storage.reactive_context.clone()
        };
        let _entered = Entered::new(new_context);
        ReactiveContext::run(reactive_context, run)
    }

    pub fn run<T>(new_context: Self, run: impl FnOnce() -> T) -> T {
        let _entered = Entered::new(new_context);
        run()
    }

    /// Run a closure using `scope_id` as the current scope, restoring the previous one afterwards.
    ///
    /// The closure still runs when there is no current context, just without any scope change.
    pub(crate) fn run_in_scope<T>(scope_id: ScopeId, run: impl FnOnce() -> T) -> T {
        let _in_scope = InScope::new(scope_id);
        run()
    }

    pub fn with<T>(with: impl FnOnce(&CurrentContext) -> T) -> T {
        CURRENT_CONTEXT.with(|context| with(context.borrow().as_ref().expect("Your trying to access Freya's current context outside of it, you might be in a separate thread or async task that is not integrated with Freya.")))
    }

    pub fn try_with<T>(with: impl FnOnce(&CurrentContext) -> T) -> Option<T> {
        CURRENT_CONTEXT
            .try_with(|context| {
                if let Ok(context) = context.try_borrow()
                    && let Some(context) = context.as_ref()
                {
                    Some(with(context))
                } else {
                    None
                }
            })
            .ok()
            .flatten()
    }
}

/// Makes a context current until dropped, which happens even when the code under it unwinds,
/// so a host that catches the panic does not run later code under it.
struct Entered;

impl Entered {
    fn new(context: CurrentContext) -> Self {
        CURRENT_CONTEXT.with_borrow_mut(|current| current.replace(context));
        Entered
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with_borrow_mut(|current| current.take());
    }
}

/// Makes a scope the current context's scope until dropped, then restores the one before it.
struct InScope(Option<ScopeId>);

impl InScope {
    fn new(scope_id: ScopeId) -> Self {
        InScope(CURRENT_CONTEXT.with_borrow_mut(|context| {
            context
                .as_mut()
                .map(|context| std::mem::replace(&mut context.scope_id, scope_id))
        }))
    }
}

impl Drop for InScope {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with_borrow_mut(|context| {
            if let Some(context) = context.as_mut()
                && let Some(previous_scope_id) = self.0
            {
                context.scope_id = previous_scope_id;
            }
        });
    }
}

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<CurrentContext>> = const { RefCell::new(None) }
}
