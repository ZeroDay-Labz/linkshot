//! A generic undo/redo stack of reversible commands.

pub trait Command<T>: Send {
    fn name(&self) -> &str;
    fn apply(&mut self, target: &mut T);
    fn revert(&mut self, target: &mut T);
}

pub struct UndoStack<T> {
    done: Vec<Box<dyn Command<T>>>,
    undone: Vec<Box<dyn Command<T>>>,
    pub limit: usize,
}

impl<T> Default for UndoStack<T> {
    fn default() -> Self { Self { done: vec![], undone: vec![], limit: 200 } }
}

impl<T> UndoStack<T> {
    pub fn push(&mut self, mut cmd: Box<dyn Command<T>>, target: &mut T) {
        cmd.apply(target);
        self.done.push(cmd);
        self.undone.clear();
        if self.done.len() > self.limit { self.done.remove(0); }
    }
    pub fn undo(&mut self, target: &mut T) -> bool {
        match self.done.pop() {
            Some(mut c) => { c.revert(target); self.undone.push(c); true }
            None => false,
        }
    }
    pub fn redo(&mut self, target: &mut T) -> bool {
        match self.undone.pop() {
            Some(mut c) => { c.apply(target); self.done.push(c); true }
            None => false,
        }
    }
    pub fn undo_all(&mut self, target: &mut T) {
        while self.undo(target) {}
    }
    pub fn can_undo(&self) -> bool { !self.done.is_empty() }
    pub fn can_redo(&self) -> bool { !self.undone.is_empty() }
    pub fn undo_name(&self) -> Option<&str> { self.done.last().map(|c| c.name()) }
    pub fn redo_name(&self) -> Option<&str> { self.undone.last().map(|c| c.name()) }
    pub fn clear(&mut self) { self.done.clear(); self.undone.clear(); }
    pub fn len(&self) -> usize { self.done.len() }
    pub fn is_empty(&self) -> bool { self.done.is_empty() }
    pub fn names(&self) -> Vec<&str> { self.done.iter().map(|c| c.name()).collect() }
}

/// A command built from a snapshot of the whole target (simple and always correct; fine for documents).
pub struct SnapshotCommand<T: Clone + Send> {
    name: String,
    before: Option<T>,
    after: Option<T>,
}
impl<T: Clone + Send> SnapshotCommand<T> {
    /// `before` is the state prior to the edit; `after` the state once edited.
    pub fn new(name: impl Into<String>, before: T, after: T) -> Self {
        Self { name: name.into(), before: Some(before), after: Some(after) }
    }
}
impl<T: Clone + Send> Command<T> for SnapshotCommand<T> {
    fn name(&self) -> &str { &self.name }
    fn apply(&mut self, target: &mut T) { if let Some(a) = &self.after { *target = a.clone(); } }
    fn revert(&mut self, target: &mut T) { if let Some(b) = &self.before { *target = b.clone(); } }
}
