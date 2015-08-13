use std::rt;

pub struct Options {
    pub stack_size: usize,
    pub name: Option<String>,
}

impl Options {
    pub fn new() -> Options {
        Options {
            stack_size: rt::min_stack(),
            name: None,
        }
    }

    pub fn stack_size(mut self, size: usize) -> Options {
        self.stack_size = size;
        self
    }

    pub fn name(mut self, name: Option<String>) -> Options {
        self.name = name;
        self
    }
}
