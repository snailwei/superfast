//! Stacked value — current value with a stack of previous values.

#[derive(Debug, PartialEq)]
pub struct Stacked<T> {
    pub(crate) current: Option<T>,
    pub(crate) stack: Vec<T>,
}

impl<T> Stacked<T> {
    pub fn new_empty() -> Self {
        Self {
            current: None,
            stack: Vec::new(),
        }
    }

    pub fn new(v: T) -> Self {
        Self {
            current: Some(v),
            stack: Vec::new(),
        }
    }

    pub fn push(&mut self, v: T) {
        match self.current.replace(v) {
            None => {}
            Some(old) => {
                self.stack.push(old);
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let old = self.stack.pop();
        match old {
            None => self.current.take(),
            Some(v) => self.current.replace(v),
        }
    }

    #[inline]
    pub fn peek(&self) -> Option<&T> {
        self.current.as_ref()
    }

    #[inline]
    pub fn must_peek(&self) -> &T {
        self.peek().unwrap()
    }

    #[inline]
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.current.as_mut()
    }

    #[inline]
    pub fn must_peek_mut(&mut self) -> &mut T {
        self.peek_mut().unwrap()
    }
}
