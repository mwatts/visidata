use crate::sheet::Sheet;

/// A stack of sheets, representing the navigation history.
///
/// The top of the stack is the active (displayed) sheet. Pushing a new sheet
/// navigates into it; popping returns to the previous sheet.
#[derive(Debug, Default)]
pub struct SheetStack {
    sheets: Vec<Sheet>,
}

impl SheetStack {
    /// Create a new empty sheet stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a sheet onto the stack, making it the active sheet.
    pub fn push(&mut self, sheet: Sheet) {
        self.sheets.push(sheet);
    }

    /// Pop the top sheet off the stack, returning it.
    ///
    /// Returns `None` if the stack is empty.
    pub fn pop(&mut self) -> Option<Sheet> {
        self.sheets.pop()
    }

    /// Returns a reference to the active (top) sheet.
    #[must_use]
    pub fn active(&self) -> Option<&Sheet> {
        self.sheets.last()
    }

    /// Returns a mutable reference to the active (top) sheet.
    pub fn active_mut(&mut self) -> Option<&mut Sheet> {
        self.sheets.last_mut()
    }

    /// Returns the number of sheets on the stack.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.sheets.len()
    }

    /// Returns `true` if there are no sheets on the stack.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    /// Returns an iterator over all sheets (bottom to top).
    pub fn iter(&self) -> impl Iterator<Item = &Sheet> {
        self.sheets.iter()
    }

    /// Returns a slice of all sheets.
    #[must_use]
    pub fn as_slice(&self) -> &[Sheet] {
        &self.sheets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn push_and_pop() {
        let mut stack = SheetStack::new();
        assert!(stack.is_empty());

        let mut s1 = Sheet::new("first");
        s1.add_row(vec![Value::Int(1)]);
        stack.push(s1);

        assert_eq!(stack.len(), 1);
        assert_eq!(stack.active().unwrap().name, "first");

        stack.push(Sheet::new("second"));
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.active().unwrap().name, "second");

        let popped = stack.pop().unwrap();
        assert_eq!(popped.name, "second");
        assert_eq!(stack.active().unwrap().name, "first");
    }

    #[test]
    fn pop_empty() {
        let mut stack = SheetStack::new();
        assert!(stack.pop().is_none());
    }

    #[test]
    fn active_mut() {
        let mut stack = SheetStack::new();
        stack.push(Sheet::new("test"));
        stack.active_mut().unwrap().add_column("col", 0);
        assert_eq!(stack.active().unwrap().num_cols(), 1);
    }

    #[test]
    fn iterate() {
        let mut stack = SheetStack::new();
        stack.push(Sheet::new("a"));
        stack.push(Sheet::new("b"));
        stack.push(Sheet::new("c"));

        let names: Vec<_> = stack.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn as_slice() {
        let mut stack = SheetStack::new();
        stack.push(Sheet::new("x"));
        stack.push(Sheet::new("y"));
        let slice = stack.as_slice();
        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0].name, "x");
        assert_eq!(slice[1].name, "y");
    }

    #[test]
    fn active_on_empty() {
        let stack = SheetStack::new();
        assert!(stack.active().is_none());
    }

    #[test]
    fn active_mut_on_empty() {
        let mut stack = SheetStack::new();
        assert!(stack.active_mut().is_none());
    }

    #[test]
    fn pop_all() {
        let mut stack = SheetStack::new();
        stack.push(Sheet::new("a"));
        stack.push(Sheet::new("b"));
        stack.pop();
        stack.pop();
        assert!(stack.is_empty());
        assert!(stack.active().is_none());
    }
}
